defmodule TheGathering.Games.SummaryImage do
  @moduledoc false

  alias TheGathering.Catalog
  alias TheGathering.Games.SummaryCard

  @max_art_bytes 2_000_000

  def render(game) do
    case System.find_executable("rsvg-convert") do
      nil -> {:error, :renderer_unavailable}
      executable -> rasterize(executable, SummaryCard.svg(game, artwork(game)))
    end
  rescue
    _error in [File.Error, ErlangError] -> {:error, :render_failed}
  end

  defp artwork(game) do
    refs =
      for %{deck: deck} <- game.seats,
          deck != nil,
          {id, name, printing} <- [
            {deck.commander_card_id, deck.commander_name, deck.commander_printing_id},
            {deck.partner_card_id, deck.partner_name, deck.partner_printing_id}
          ],
          name != nil,
          do: {id, name, printing}

    urls =
      Catalog.art_crop_urls(
        Enum.flat_map(refs, fn {id, name, printing} -> [{id, name}, {:printing, printing}] end)
      )

    refs
    |> Enum.map(fn {id, name, printing} = ref ->
      {ref, Catalog.art_crop_url(urls, id, name, printing)}
    end)
    |> Task.async_stream(fn {ref, url} -> {ref, fetch_art(url)} end,
      max_concurrency: 6,
      timeout: 6_000,
      on_timeout: :kill_task
    )
    |> Enum.flat_map(fn
      {:ok, {ref, image}} when is_binary(image) -> [{ref, image}]
      _ -> []
    end)
    |> Map.new()
  end

  # Imported catalog URLs are not trusted. Never fetch arbitrary hosts, redirects,
  # local files or SVGs; missing art simply uses the card's built-in fallback.
  def fetch_art(url, request_options \\ [])

  def fetch_art(url, request_options) when is_binary(url) do
    case URI.parse(url) do
      %URI{scheme: "https", host: "cards.scryfall.io", port: 443, userinfo: nil} ->
        download(url, request_options)

      _ ->
        nil
    end
  end

  def fetch_art(_url, _request_options), do: nil

  defp download(url, request_options) do
    options =
      Keyword.merge(request_options,
        retry: false,
        redirect: false,
        decode_body: false,
        connect_options: [timeout: 2_000],
        receive_timeout: 3_000,
        into: &collect_art/2
      )

    case Req.get(url, options) do
      {:ok, %{status: 200, body: <<255, 216, 255, _::binary>> = body}} ->
        data_uri("jpeg", body)

      {:ok, %{status: 200, body: <<137, 80, 78, 71, 13, 10, 26, 10, _::binary>> = body}} ->
        data_uri("png", body)

      _ ->
        nil
    end
  end

  defp collect_art({:data, chunk}, {request, response}) do
    body = (response.body || "") <> chunk

    if byte_size(body) <= @max_art_bytes,
      do: {:cont, {request, %{response | body: body}}},
      else: {:halt, {request, %{response | status: 413, body: ""}}}
  end

  defp data_uri(type, bytes), do: "data:image/#{type};base64," <> Base.encode64(bytes)

  defp rasterize(executable, svg) do
    directory = Path.join(System.tmp_dir!(), "gathering-summary-" <> Ecto.UUID.generate())
    File.mkdir!(directory)

    try do
      input = Path.join(directory, "card.svg")
      output = Path.join(directory, "card.png")
      File.write!(input, svg)

      case System.cmd(executable, ["--output", output, input], stderr_to_stdout: true) do
        {_message, 0} -> File.read(output)
        _ -> {:error, :render_failed}
      end
    after
      File.rm_rf(directory)
    end
  end
end
