defmodule TheGathering.Catalog.Scryfall do
  @moduledoc false

  @bulk_url "https://api.scryfall.com/bulk-data"
  @user_agent "TheGathering/0.1 (+https://github.com/cfbender/the-gathering)"
  # Every seat at a webcam table looks up a newly identified card at the same moment, so
  # card lookups queue for the shared limit briefly instead of failing all but the first.
  @card_queue_ms 3_000

  def printings(oracle_id, page) do
    limit = Application.get_env(:the_gathering, :scryfall_search_limit, 1)

    with {:allow, _count} <- TheGathering.RateLimiter.hit(:scryfall_search, 500, limit),
         {:ok, response} <- search_printings(oracle_id, page) do
      case response do
        %{status: 200, body: %{"data" => cards, "has_more" => has_more}}
        when is_list(cards) and is_boolean(has_more) ->
          {:ok, cards, has_more}

        %{status: 404} ->
          {:ok, [], false}

        _other ->
          {:error, :bad_gateway}
      end
    else
      _error -> {:error, :bad_gateway}
    end
  end

  @doc "Fetches one printing by Scryfall id, for card details the catalog does not hold."
  def card(id) when is_binary(id) do
    limit = Application.get_env(:the_gathering, :scryfall_search_limit, 1)
    deadline = System.monotonic_time(:millisecond) + @card_queue_ms

    with :ok <- await_slot(:scryfall_card, 100, limit, deadline),
         {:ok, response} <- Req.get("https://api.scryfall.com/cards/#{id}", request_options()) do
      case response do
        %{status: 200, body: %{"id" => _id} = card} -> {:ok, card}
        %{status: 404} -> {:error, :not_found}
        _other -> {:error, :bad_gateway}
      end
    else
      _error -> {:error, :bad_gateway}
    end
  end

  def rulings(id) when is_binary(id) do
    limit = Application.get_env(:the_gathering, :scryfall_search_limit, 1)

    with {:allow, _count} <- TheGathering.RateLimiter.hit(:scryfall_rulings, 100, limit),
         {:ok, response} <-
           Req.get(
             "https://api.scryfall.com/cards/#{URI.encode_www_form(id)}/rulings",
             request_options()
           ) do
      case response do
        %{status: 200, body: %{"data" => rulings}} when is_list(rulings) ->
          {:ok, Enum.map(rulings, &Map.take(&1, ["source", "published_at", "comment"]))}

        %{status: 404} ->
          {:error, :not_found}

        _other ->
          {:error, :bad_gateway}
      end
    else
      _error -> {:error, :bad_gateway}
    end
  end

  defp await_slot(key, scale, limit, deadline) do
    case TheGathering.RateLimiter.hit(key, scale, limit) do
      {:allow, _count} ->
        :ok

      {:deny, retry_after} ->
        if System.monotonic_time(:millisecond) + retry_after <= deadline do
          Process.sleep(retry_after)
          await_slot(key, scale, limit, deadline)
        else
          {:error, :rate_limited}
        end
    end
  end

  defp search_printings(oracle_id, page) do
    options =
      request_options(
        params: [
          q: "oracleid:#{oracle_id} game:paper lang:en",
          unique: "prints",
          order: "released",
          include_variations: true,
          page: page
        ]
      )

    Req.get("https://api.scryfall.com/cards/search", options)
  end

  defp request_options(extra \\ []) do
    [
      headers: headers(),
      connect_options: [timeout: 3_000],
      receive_timeout: 10_000,
      retry: false
    ]
    |> Keyword.merge(extra)
    |> Keyword.merge(Application.get_env(:the_gathering, :scryfall_req_options, []))
  end

  def fetch do
    response = Req.get!(@bulk_url, headers: headers())

    metadata =
      Enum.find(response.body["data"], &(&1["type"] == "default_cards")) ||
        raise "Scryfall did not return default_cards bulk metadata"

    uri = metadata["jsonl_download_uri"] || raise "Scryfall default_cards has no JSONL URI"

    path =
      Path.join(
        System.tmp_dir!(),
        "the-gathering-scryfall-#{System.unique_integer([:positive])}.jsonl.gz"
      )

    download =
      Req.get!(uri,
        headers: headers(),
        into: File.stream!(path),
        decode_body: false,
        receive_timeout: 30 * 60 * 1_000
      )

    if download.status != 200 do
      File.rm(path)
      raise "Scryfall bulk download returned HTTP #{download.status}"
    end

    %{path: path, updated_at: parse_datetime(metadata["updated_at"]), temporary?: true}
  end

  defp headers, do: [{"user-agent", @user_agent}, {"accept", "application/json"}]

  defp parse_datetime(value) do
    case DateTime.from_iso8601(value || "") do
      {:ok, datetime, _offset} -> DateTime.truncate(datetime, :second)
      _error -> nil
    end
  end
end
