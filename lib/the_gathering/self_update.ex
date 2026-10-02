defmodule TheGathering.SelfUpdate do
  @moduledoc """
  Lets an administrator update the server from the admin UI.

  The app never replaces itself; it hands the job to whatever runs it, and that updater decides
  what "newest" means for the installed channel:

    * systemd (the Proxmox LXC from `deploy/proxmox/the-gathering.sh`): `SELF_UPDATE_REQUEST_FILE`
      names a file under `DATA_DIR` that a `the-gathering-update.path` unit watches. Creating it
      makes systemd run `update` as root, which installs the newest build of the installed channel
      (tagged release or nightly) and restarts the service.
    * Watchtower (Docker): `WATCHTOWER_HTTP_API_TOKEN` (and optionally `WATCHTOWER_URL`,
      `WATCHTOWER_IMAGE`) point at a Watchtower container with its update endpoint enabled. It
      pulls the image tag the container was started from and recreates the container.

  The running version is `priv/VERSION`, written by the release and container builds: `vX.Y.Z`
  for tagged releases, `nightly-<commit>` for builds of `main`. GitHub says what the newest one
  is; that answer is cached for a while because the API allows 60 anonymous requests an hour.
  """

  use GenServer

  require Logger

  @repo "cfbender/the-gathering"
  @github_api "https://api.github.com/repos/#{@repo}"
  @nightly_url "https://github.com/#{@repo}/releases/tag/nightly"
  @default_image "ghcr.io/#{@repo}"
  @default_watchtower_url "http://watchtower:8080"
  @check_ttl_ms :timer.minutes(15)
  # A request older than this without a restart has most likely failed; let the admin retry.
  @pending_ms :timer.minutes(15)

  @type channel :: :release | :nightly
  @type method :: :systemd | :watchtower

  @type status :: %{
          version: String.t() | nil,
          channel: channel | nil,
          method: method | nil,
          pending: boolean,
          requested_at: DateTime.t() | nil,
          latest: %{version: String.t(), url: String.t()} | nil,
          update_available: boolean | nil,
          check_error: String.t() | nil
        }

  def start_link(_options), do: GenServer.start_link(__MODULE__, :ok, name: __MODULE__)

  @doc "The running version from `priv/VERSION`, or nil for a development build."
  @spec version() :: String.t() | nil
  def version do
    case File.read(version_file()) do
      {:ok, contents} ->
        case String.trim(contents) do
          "" -> nil
          version -> version
        end

      {:error, _reason} ->
        nil
    end
  end

  @doc "Which stream of builds the running version came from."
  @spec channel(String.t() | nil) :: channel | nil
  def channel(version \\ version())
  def channel("v" <> _rest), do: :release
  def channel("nightly" <> _rest), do: :nightly
  def channel(_other), do: nil

  @doc "How this server can be updated, or nil when no updater is configured."
  @spec method() :: method | nil
  def method do
    cond do
      present?(config(:request_file)) -> :systemd
      present?(config(:watchtower_token)) -> :watchtower
      true -> nil
    end
  end

  @doc "Version, channel, updater, whether an update is pending, and what GitHub has newest."
  @spec status() :: status
  def status, do: GenServer.call(__MODULE__, :status, 20_000)

  @doc """
  Asks the configured updater to install the newest build of the installed channel.

  Returns the new status (with `pending: true`) or `{:error, :unsupported}` when no updater is
  configured, `{:error, :update_in_progress}` when Watchtower is already updating, and
  `{:error, :updater_unavailable}` when the request file cannot be written or Watchtower fails.
  """
  @spec request_update() ::
          {:ok, status} | {:error, :unsupported | :update_in_progress | :updater_unavailable}
  def request_update, do: GenServer.call(__MODULE__, :request_update, 60_000)

  @doc false
  def reset, do: GenServer.call(__MODULE__, :reset)

  @impl true
  def init(:ok), do: {:ok, %{latest: nil, requested_at: nil}}

  @impl true
  def handle_call(:status, _from, state) do
    state = refresh_latest(state)
    {:reply, build_status(state), state}
  end

  def handle_call(:request_update, _from, state) do
    case request(method()) do
      :ok ->
        state = %{state | requested_at: DateTime.utc_now()}
        {:reply, {:ok, build_status(state)}, state}

      {:error, reason} ->
        {:reply, {:error, reason}, state}
    end
  end

  def handle_call(:reset, _from, _state), do: {:reply, :ok, %{latest: nil, requested_at: nil}}

  defp build_status(state) do
    version = version()
    channel = channel(version)
    {latest, check_error} = latest_result(state, channel)

    %{
      version: version,
      channel: channel,
      method: method(),
      pending: pending?(state),
      requested_at: state.requested_at,
      latest: latest,
      update_available: latest && update_available?(channel, version, latest.version),
      check_error: check_error
    }
  end

  defp latest_result(%{latest: {channel, result, _checked_at}}, channel) do
    case result do
      {:ok, latest} -> {latest, nil}
      {:error, message} -> {nil, message}
    end
  end

  defp latest_result(_state, _channel), do: {nil, nil}

  # systemd removes the request file once `update` has finished, so the file alone says whether the
  # update is still running, even when it failed without a restart. Watchtower gives no such signal;
  # a recent request counts as pending until the container has been replaced.
  defp pending?(state) do
    case method() do
      :systemd ->
        File.exists?(config(:request_file))

      _method ->
        state.requested_at != nil and
          DateTime.diff(DateTime.utc_now(), state.requested_at, :millisecond) < @pending_ms
    end
  end

  # -- newest version on GitHub -------------------------------------------------------------------

  defp refresh_latest(state) do
    channel = channel()
    now = System.monotonic_time(:millisecond)
    stale_before = now - check_ttl_ms()

    case state.latest do
      {^channel, _result, checked_at} when checked_at > stale_before -> state
      _stale when is_nil(channel) -> %{state | latest: nil}
      _stale -> %{state | latest: {channel, fetch_latest(channel), now}}
    end
  end

  defp fetch_latest(:release) do
    case github_get("/releases/latest") do
      {:ok, %{"tag_name" => "v" <> _ = tag, "html_url" => url}} when is_binary(url) ->
        {:ok, %{version: tag, url: url}}

      {:ok, body} ->
        unexpected_response(body)

      {:error, _reason} = error ->
        error
    end
  end

  defp fetch_latest(:nightly) do
    case github_get("/git/ref/tags/nightly") do
      {:ok, %{"object" => %{"sha" => sha}}} when is_binary(sha) ->
        {:ok, %{version: "nightly-" <> String.slice(sha, 0, 7), url: @nightly_url}}

      {:ok, body} ->
        unexpected_response(body)

      {:error, _reason} = error ->
        error
    end
  end

  defp github_get(path) do
    options =
      [
        headers: [accept: "application/vnd.github+json", "x-github-api-version": "2022-11-28"],
        connect_options: [timeout: 3_000],
        receive_timeout: 5_000,
        retry: false
      ]
      |> Keyword.merge(config(:req_options, []))

    case Req.get(@github_api <> path, options) do
      {:ok, %Req.Response{status: 200, body: body}} when is_map(body) ->
        {:ok, body}

      {:ok, %Req.Response{status: 403} = response} ->
        if rate_limited?(response),
          do: {:error, "GitHub's API rate limit was reached; try again later."},
          else: {:error, "GitHub answered with status 403."}

      {:ok, %Req.Response{status: status}} ->
        {:error, "GitHub answered with status #{status}."}

      {:error, reason} ->
        Logger.warning("Update check against GitHub failed: #{inspect(reason)}")
        {:error, "Could not reach GitHub."}
    end
  end

  defp rate_limited?(response) do
    Req.Response.get_header(response, "x-ratelimit-remaining") == ["0"]
  end

  defp unexpected_response(body) do
    Logger.warning("Update check got an unexpected GitHub response: #{inspect(body)}")
    {:error, "GitHub returned an unexpected response."}
  end

  @doc false
  @spec update_available?(channel, String.t(), String.t()) :: boolean
  def update_available?(:release, "v" <> current, "v" <> latest) do
    case {Version.parse(current), Version.parse(latest)} do
      {{:ok, current}, {:ok, latest}} -> Version.compare(latest, current) == :gt
      _unparsable -> current != latest
    end
  end

  def update_available?(:nightly, "nightly-" <> current, "nightly-" <> latest)
      when current != "" and latest != "" do
    not (String.starts_with?(latest, current) or String.starts_with?(current, latest))
  end

  def update_available?(_channel, current, latest), do: current != latest

  # -- asking the updater -------------------------------------------------------------------------

  defp request(:systemd) do
    path = config(:request_file)

    case File.write(path, DateTime.to_iso8601(DateTime.utc_now()) <> "\n") do
      :ok ->
        Logger.info("Update requested; wrote #{path} for the-gathering-update.path")
        :ok

      {:error, reason} ->
        Logger.error("Could not write the update request file #{path}: #{inspect(reason)}")
        {:error, :updater_unavailable}
    end
  end

  defp request(:watchtower) do
    url = present_or(config(:watchtower_url), @default_watchtower_url)

    options =
      [
        params: [image: present_or(config(:watchtower_image), @default_image), async: true],
        auth: {:bearer, config(:watchtower_token)},
        connect_options: [timeout: 3_000],
        # Older Watchtower builds ignore `async` and only answer once the update has finished,
        # which is after this container has been replaced; a long wait is harmless.
        receive_timeout: 30_000,
        retry: false
      ]
      |> Keyword.merge(config(:req_options, []))

    case Req.post(url <> "/v1/update", options) do
      {:ok, %Req.Response{status: status}} when status in [200, 202] ->
        Logger.info("Update requested from Watchtower at #{url}")
        :ok

      {:ok, %Req.Response{status: 429}} ->
        {:error, :update_in_progress}

      {:ok, %Req.Response{status: status, body: body}} ->
        Logger.error("Watchtower refused the update request with #{status}: #{inspect(body)}")
        {:error, :updater_unavailable}

      {:error, reason} ->
        Logger.error("Watchtower at #{url} could not be reached: #{inspect(reason)}")
        {:error, :updater_unavailable}
    end
  end

  defp request(nil), do: {:error, :unsupported}

  # -- configuration ------------------------------------------------------------------------------

  defp version_file do
    present_or(config(:version_file), Application.app_dir(:the_gathering, "priv/VERSION"))
  end

  defp check_ttl_ms, do: config(:check_ttl_ms, @check_ttl_ms)

  defp config(key, default \\ nil) do
    :the_gathering |> Application.get_env(__MODULE__, []) |> Keyword.get(key, default)
  end

  defp present?(value), do: is_binary(value) and value != ""
  defp present_or(value, default), do: if(present?(value), do: value, else: default)
end
