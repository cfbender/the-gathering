defmodule TheGathering.Catalog.CardImages do
  @moduledoc "Bounded, shared disk cache for unchanged Scryfall JPEGs. Never fetches arbitrary origins."
  use GenServer

  @source ~r/\Ahttps:\/\/cards\.scryfall\.io\/(small|normal|art_crop)\/(front|back)\/[0-9a-f]\/[0-9a-f]\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.jpg(?:\?[0-9]+)?\z/
  @ttl 30 * 24 * 60 * 60
  @max_image_bytes 2 * 1024 * 1024
  @max_bytes 512 * 1024 * 1024
  @concurrency 4
  @max_pending 128

  def start_link(opts), do: GenServer.start_link(__MODULE__, opts, name: __MODULE__)

  def url(source) when is_binary(source) do
    if valid_source?(source),
      do: "/api/card-images?" <> URI.encode_query(%{url: source}),
      else: source
  end

  def url(nil), do: nil
  def urls(images), do: Map.new(images || %{}, fn {variant, source} -> {variant, url(source)} end)

  @doc """
  Cached `small` and `normal` front images of one exact printing, derived from its Scryfall id
  (Scryfall's CDN serves `/<variant>/front/<a>/<b>/<id>.jpg` without the cache-busting query).
  `nil` for anything that is not a printing UUID.
  """
  def printing_urls(<<a, b, _::binary>> = id) do
    if Regex.match?(~r/\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z/, id) do
      Map.new(~w(small normal), fn variant ->
        {variant, url("https://cards.scryfall.io/#{variant}/front/#{<<a>>}/#{<<b>>}/#{id}.jpg")}
      end)
    end
  end

  def printing_urls(_id), do: nil

  @doc "Inverse of `url/1`: the Scryfall source behind a `/api/card-images` URL, or the input unchanged."
  def source("/api/card-images?" <> query) do
    case URI.decode_query(query) do
      %{"url" => source} when is_binary(source) -> source
      _ -> nil
    end
  end

  def source(url), do: url

  def fetch(source) do
    if valid_source?(source),
      do: GenServer.call(__MODULE__, {:fetch, source}, :infinity),
      else: {:error, :bad_request}
  end

  defp valid_source?(source), do: is_binary(source) and Regex.match?(@source, source)
  defp key(source), do: :crypto.hash(:sha256, source) |> Base.encode16(case: :lower)

  @impl true
  def init(_opts) do
    root = Path.join(Application.fetch_env!(:the_gathering, :data_dir), "card-images")
    File.mkdir_p!(root)

    entries =
      for path <- Path.wildcard(Path.join(root, "*.jpg")),
          {:ok, stat} <- [File.stat(path, time: :posix)],
          into: %{} do
        {Path.basename(path, ".jpg"), {stat.size, stat.mtime}}
      end

    {:ok,
     prune(%{
       root: root,
       entries: entries,
       pending: %{},
       tasks: %{},
       queue: :queue.new(),
       paused_until: 0
     })}
  end

  @impl true
  def handle_call({:fetch, source}, from, state) do
    id = key(source)

    case cached(state, id) do
      {:ok, body} -> {:reply, {:ok, body, "hit"}, state}
      :miss -> enqueue(state, id, source, from)
    end
  end

  defp cached(state, id) do
    with {_size, created} <- Map.get(state.entries, id),
         true <- System.os_time(:second) - created < @ttl,
         {:ok, body} <- File.read(path(state, id)) do
      {:ok, body}
    else
      _ -> :miss
    end
  end

  defp enqueue(state, id, source, from) do
    cond do
      state.paused_until > System.os_time(:second) ->
        {:reply, {:error, :bad_gateway}, state}

      Map.has_key?(state.pending, id) ->
        {:noreply, update_in(state.pending[id], &[from | &1])}

      map_size(state.pending) >= @max_pending ->
        {:reply, {:error, :bad_gateway}, state}

      true ->
        state = %{
          state
          | pending: Map.put(state.pending, id, [from]),
            queue: :queue.in({id, source}, state.queue)
        }

        {:noreply, dispatch(state)}
    end
  end

  defp dispatch(state) when map_size(state.tasks) >= @concurrency, do: state

  defp dispatch(state) do
    case :queue.out(state.queue) do
      {:empty, _} ->
        state

      {{:value, {id, source}}, queue} ->
        task =
          Task.Supervisor.async_nolink(TheGathering.Catalog.TaskSupervisor, fn ->
            download(source)
          end)

        dispatch(%{state | queue: queue, tasks: Map.put(state.tasks, task.ref, id)})
    end
  end

  @impl true
  def handle_info({ref, result}, state) when is_reference(ref) do
    Process.demonitor(ref, [:flush])
    {:noreply, finish(state, ref, result)}
  end

  def handle_info({:DOWN, ref, :process, _pid, _reason}, state),
    do: {:noreply, finish(state, ref, {:error, :bad_gateway})}

  defp finish(state, ref, result) do
    {id, tasks} = Map.pop(state.tasks, ref)
    {callers, pending} = Map.pop(state.pending, id, [])
    state = %{state | tasks: tasks, pending: pending}

    {reply, state} =
      case result do
        {:ok, body} ->
          state = store(state, id, body)
          {{:ok, body, "miss"}, state}

        {:error, {:rate_limited, seconds}} ->
          {{:error, :bad_gateway}, pause(state, seconds)}

        error ->
          {error, state}
      end

    Enum.each(callers, &GenServer.reply(&1, reply))
    dispatch(state)
  end

  defp pause(state, seconds) do
    pending =
      Enum.reduce(:queue.to_list(state.queue), state.pending, fn {id, _url}, pending ->
        {callers, pending} = Map.pop(pending, id, [])
        Enum.each(callers, &GenServer.reply(&1, {:error, :bad_gateway}))
        pending
      end)

    %{
      state
      | pending: pending,
        queue: :queue.new(),
        paused_until: System.os_time(:second) + seconds
    }
  end

  defp download(source) do
    options = [
      headers: [
        {"user-agent", "TheGathering/0.1 (+https://github.com/cfbender/the-gathering)"},
        {"accept", "image/jpeg"}
      ],
      redirect: false,
      retry: false,
      decode_body: false,
      connect_options: [timeout: 3_000],
      receive_timeout: 10_000,
      into: &receive_chunk/2
    ]

    options =
      Keyword.merge(options, Application.get_env(:the_gathering, :card_image_req_options, []))

    case Req.get(source, options) do
      {:ok, %{status: 200, body: <<255, 216, 255, _::binary>> = body}}
      when byte_size(body) <= @max_image_bytes ->
        {:ok, body}

      {:ok, %{status: 404}} ->
        {:error, :not_found}

      {:ok, %{status: 429} = response} ->
        seconds =
          case Integer.parse(List.first(Req.Response.get_header(response, "retry-after")) || "30") do
            {seconds, ""} -> max(seconds, 30)
            _ -> 30
          end

        {:error, {:rate_limited, seconds}}

      _ ->
        {:error, :bad_gateway}
    end
  end

  defp receive_chunk({:data, chunk}, {request, response}) do
    body = (response.body || "") <> chunk

    if byte_size(body) > @max_image_bytes,
      do: {:halt, {request, %{response | status: 502, body: ""}}},
      else: {:cont, {request, %{response | body: body}}}
  end

  defp store(state, id, body) do
    target = path(state, id)

    with :ok <- File.write(target <> ".tmp", body),
         :ok <- File.rename(target <> ".tmp", target) do
      prune(%{
        state
        | entries: Map.put(state.entries, id, {byte_size(body), System.os_time(:second)})
      })
    else
      _ ->
        File.rm(target <> ".tmp")
        state
    end
  end

  defp prune(state) do
    total = Enum.reduce(state.entries, 0, fn {_id, {size, _time}}, sum -> sum + size end)

    {entries, _total} =
      state.entries
      |> Enum.sort_by(fn {_id, {_size, time}} -> time end)
      |> Enum.reduce({state.entries, total}, fn {id, {size, time}}, {entries, bytes} ->
        if bytes > @max_bytes or System.os_time(:second) - time >= @ttl do
          File.rm(path(state, id))
          {Map.delete(entries, id), bytes - size}
        else
          {entries, bytes}
        end
      end)

    %{state | entries: entries}
  end

  defp path(state, id), do: Path.join(state.root, id <> ".jpg")
end
