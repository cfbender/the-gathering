defmodule TheGathering.Decklists.RemoteDecks do
  @moduledoc "Lists and normalizes public decks from a user's configured deck hosts."

  alias TheGathering.Accounts.User
  alias TheGathering.Decklists.{Budget, Cache, HTTP}

  @moxfield_url "https://api2.moxfield.com/v2/decks/search-sfw"
  @archidekt_url "https://archidekt.com/api/decks/v3/"
  @colors ~w(W U B R G)
  @default_limits %{max_pages: 20, max_decks: 500, max_bytes: 2_000_000, duration_ms: 15_000}

  def list(%User{} = user) do
    key = {:remote_decks, user.id}
    fingerprint = settings_fingerprint(user)

    case Cache.fetch(key) do
      {:ok, {^fingerprint, result}} -> result
      _miss_or_changed -> fetch_once(key, fingerprint, user)
    end
  end

  defp fetch_once(key, fingerprint, user) do
    :global.trans({{__MODULE__, key}, self()}, fn ->
      case Cache.fetch(key) do
        {:ok, {^fingerprint, result}} -> result
        _miss_or_changed -> fetch_and_cache(key, fingerprint, user)
      end
    end)
  end

  defp fetch_and_cache(key, fingerprint, user) do
    sources = [
      fetch_source(:moxfield, user.moxfield_username, &fetch_moxfield/1),
      fetch_source(:archidekt, user.archidekt_username, &fetch_archidekt/1),
      manavault_source(user.manavault_url, user.manavault_api_key)
    ]

    result = %{
      decks: sources |> Enum.flat_map(& &1.decks) |> sort_decks(),
      sources: Enum.map(sources, &Map.delete(&1, :decks))
    }

    :ok = Cache.put(key, {fingerprint, result})
    result
  end

  defp fetch_source(source, value, _fetcher) when value in [nil, ""] do
    %{source: source, configured: false, error: nil, decks: []}
  end

  defp fetch_source(source, value, fetcher) do
    {:ok, budget} = Budget.start_link(limits())

    try do
      case fetcher.({value, budget}) do
        {:ok, decks} ->
          %{source: source, configured: true, error: nil, decks: decks}

        {:truncated, decks, message} ->
          %{source: source, configured: true, error: message, decks: decks}

        {:error, message} ->
          %{source: source, configured: true, error: message, decks: []}
      end
    after
      Budget.stop(budget)
    end
  end

  defp manavault_source(url, _api_key) when url in [nil, ""] do
    %{source: :manavault, configured: false, error: nil, decks: []}
  end

  defp manavault_source(_url, api_key) when api_key in [nil, ""] do
    %{
      source: :manavault,
      configured: true,
      error:
        "Add a ManaVault API key in Settings to list your decks. Public share links still work individually.",
      decks: []
    }
  end

  defp manavault_source(url, api_key) do
    fetch_source(:manavault, url, fn {origin, budget} ->
      fetch_manavault(origin, api_key, budget)
    end)
  end

  defp fetch_manavault(url, api_key, budget),
    do: fetch_manavault_page(url, api_key, budget, 1, [])

  defp fetch_manavault_page(url, api_key, budget, page, decks) do
    query = URI.encode_query(%{"page" => page, "per_page" => 100})
    headers = [{"authorization", "Bearer #{api_key}"}]

    url
    |> HTTP.get_remote("/api/v1/decks?#{query}", headers, budget)
    |> handle_manavault_response(url, api_key, budget, page, decks)
  end

  defp handle_manavault_response(
         {:ok, %Req.Response{status: 200, body: %{"data" => rows} = body}},
         url,
         api_key,
         budget,
         page,
         decks
       )
       when is_list(rows) do
    {normalized, deck_limit?} = add_rows(decks, rows, &manavault_deck(&1, url))
    total_pages = get_in(body, ["pagination", "total_pages"]) || page

    continue_pages(
      normalized,
      deck_limit?,
      page,
      page < total_pages and rows != [],
      fn -> fetch_manavault_page(url, api_key, budget, page + 1, normalized) end
    )
  end

  defp handle_manavault_response(
         {:ok, %Req.Response{status: 401}},
         _url,
         _key,
         _budget,
         _page,
         _decks
       ),
       do: {:error, "ManaVault rejected the API key. Create a new one in ManaVault Settings."}

  defp handle_manavault_response(
         {:ok, %Req.Response{status: 404}},
         _url,
         _key,
         _budget,
         _page,
         _decks
       ),
       do:
         {:error,
          "ManaVault has no deck API at this URL. Update ManaVault or check the instance URL."}

  defp handle_manavault_response(
         {:ok, %Req.Response{status: 429}},
         _url,
         _key,
         _budget,
         _page,
         _decks
       ),
       do: {:error, "ManaVault rate-limited the request. Try again shortly."}

  defp handle_manavault_response(
         {:error, :blocked_destination},
         _url,
         _key,
         _budget,
         _page,
         _decks
       ),
       do:
         {:error,
          "ManaVault resolved to a blocked network address. Ask the operator to allow this host."}

  defp handle_manavault_response({:error, reason}, _url, _key, _budget, _page, decks)
       when reason in [:byte_limit, :duration_limit],
       do: budget_truncated(decks, reason)

  defp handle_manavault_response(_response, _url, _key, _budget, _page, _decks),
    do: {:error, "ManaVault could not be reached. Try again shortly."}

  defp manavault_deck(row, url) do
    %{
      name: row["name"],
      commanders: row |> Map.get("commanders", []) |> Enum.map(&commander_name/1) |> compact(),
      color_identity: order_colors(row["commanderColorIdentity"] || []),
      url: row["public_share_url"] || "#{url}/decks/#{row["id"]}",
      source: :manavault,
      updated_at: row["updated_at"]
    }
  end

  defp fetch_moxfield({username, budget}), do: fetch_moxfield_page(username, budget, 1, [])

  defp fetch_moxfield_page(username, budget, page, decks) do
    query =
      URI.encode_query(%{
        "authorUserNames" => username,
        "pageNumber" => page,
        "pageSize" => 100,
        "sortType" => "Updated",
        "sortDirection" => "Descending",
        "includePinned" => "true",
        "showIllegal" => "true"
      })

    case HTTP.get_limited("#{@moxfield_url}?#{query}", budget) do
      {:ok, %Req.Response{status: 200, body: %{"data" => rows} = body}} when is_list(rows) ->
        {normalized, deck_limit?} = add_rows(decks, rows, &moxfield_deck/1)
        total_pages = body["totalPages"] || page

        continue_pages(
          normalized,
          deck_limit?,
          page,
          page < total_pages and rows != [],
          fn -> fetch_moxfield_page(username, budget, page + 1, normalized) end
        )

      {:ok, %Req.Response{status: 404}} ->
        {:error, "Moxfield user was not found."}

      {:ok, %Req.Response{status: status}} when status in [401, 403, 429] ->
        {:error, "Moxfield blocked the request. Try again later or open decks on Moxfield."}

      {:error, reason} when reason in [:byte_limit, :duration_limit] ->
        budget_truncated(decks, reason)

      _ ->
        {:error, "Moxfield could not be reached. Try again shortly."}
    end
  end

  defp moxfield_deck(row) do
    public_id = row["publicId"] || row["id"]

    %{
      name: row["name"],
      commanders: row |> Map.get("commanders", []) |> Enum.map(&commander_name/1) |> compact(),
      color_identity: order_colors(row["colorIdentity"] || row["colors"] || []),
      url: row["publicUrl"] || "https://moxfield.com/decks/#{public_id}",
      source: :moxfield,
      updated_at: row["lastUpdatedAtUtc"]
    }
  end

  defp fetch_archidekt({username, budget}) do
    query =
      URI.encode_query(%{"ownerUsername" => username, "page" => 1, "orderBy" => "-updatedAt"})

    with {status, rows, message} when status in [:ok, :truncated] <-
           fetch_archidekt_pages(
             "#{@archidekt_url}?#{query}",
             budget,
             1,
             [],
             MapSet.new()
           ),
         {:ok, decks} <- fetch_archidekt_details(rows, budget) do
      if status == :truncated, do: {:truncated, decks, message}, else: {:ok, decks}
    end
  end

  defp fetch_archidekt_pages(url, budget, page, decks, seen) do
    if MapSet.member?(seen, url) do
      {:error, "Archidekt returned invalid pagination."}
    else
      fetch_archidekt_page(url, budget, page, decks, seen)
    end
  end

  defp fetch_archidekt_page(url, budget, page, decks, seen) do
    case HTTP.get_limited(url, budget) do
      {:ok, %Req.Response{status: 200, body: %{"results" => rows} = body}} when is_list(rows) ->
        {decks, deck_limit?} = add_rows(decks, rows, & &1)

        continue_archidekt_pages(
          body["next"],
          url,
          budget,
          page,
          decks,
          seen,
          deck_limit?
        )

      {:ok, %Req.Response{status: 404}} ->
        {:error, "Archidekt user was not found."}

      {:ok, %Req.Response{status: status}} when status in [401, 403, 429] ->
        {:error, "Archidekt blocked the request. Try again later."}

      {:error, reason} when reason in [:byte_limit, :duration_limit] ->
        budget_truncated(decks, reason)

      _ ->
        {:error, "Archidekt could not be reached. Try again shortly."}
    end
  end

  defp continue_archidekt_pages(next_value, url, budget, page, decks, seen, deck_limit?) do
    next = archidekt_next_url(next_value)

    cond do
      is_nil(next) -> {:ok, Enum.reverse(decks), nil}
      deck_limit? or length(decks) >= limits().max_decks -> truncated(decks, "deck")
      page >= limits().max_pages -> truncated(decks, "page")
      true -> fetch_archidekt_pages(next, budget, page + 1, decks, MapSet.put(seen, url))
    end
  end

  defp fetch_archidekt_details(rows, budget) do
    results =
      Task.async_stream(rows, &fetch_archidekt_detail(&1, budget),
        max_concurrency: 5,
        ordered: true,
        timeout: :infinity
      )
      |> Enum.map(fn {:ok, result} -> result end)

    case Enum.find(results, &match?({:error, _message}, &1)) do
      nil -> {:ok, Enum.map(results, fn {:ok, deck} -> deck end)}
      {:error, message} -> {:error, message}
    end
  end

  defp fetch_archidekt_detail(row, budget) do
    id = row["id"]

    case HTTP.get_limited("https://archidekt.com/api/decks/#{id}/", budget) do
      {:ok, %Req.Response{status: 200, body: body}} when is_map(body) ->
        cards = body["cards"] || []
        commanders = Enum.filter(cards, &("Commander" in (&1["categories"] || [])))

        deck = %{
          name: row["name"] || body["name"],
          commanders: Enum.map(commanders, &get_in(&1, ["card", "oracleCard", "name"])),
          color_identity: archidekt_colors(commanders),
          url: "https://archidekt.com/decks/#{id}",
          source: :archidekt,
          updated_at: row["updatedAt"]
        }

        {:ok, deck}

      {:error, reason} when reason in [:byte_limit, :duration_limit] ->
        {:error, budget_message(reason)}

      _ ->
        {:error, "Archidekt deck details could not be reached. Try again shortly."}
    end
  end

  defp archidekt_next_url(nil), do: nil

  defp archidekt_next_url(next) when is_binary(next) do
    uri = URI.parse(next)

    cond do
      is_nil(uri.host) and String.starts_with?(next, "/") -> "https://archidekt.com#{next}"
      String.downcase(uri.host || "") in ["archidekt.com", "www.archidekt.com"] -> next
      true -> nil
    end
  end

  defp archidekt_next_url(_next), do: nil

  defp archidekt_colors(commanders) do
    commanders
    |> Enum.flat_map(&(get_in(&1, ["card", "oracleCard", "colorIdentity"]) || []))
    |> Enum.map(
      &Map.get(
        %{"White" => "W", "Blue" => "U", "Black" => "B", "Red" => "R", "Green" => "G"},
        &1,
        &1
      )
    )
    |> order_colors()
  end

  defp commander_name(%{"name" => name}), do: name
  defp commander_name(%{"card" => %{"name" => name}}), do: name
  defp commander_name(%{"card" => %{"oracleCard" => %{"name" => name}}}), do: name
  defp commander_name(name) when is_binary(name), do: name
  defp commander_name(_commander), do: nil

  defp compact(values), do: Enum.reject(values, &is_nil/1)
  defp order_colors(colors), do: Enum.filter(@colors, &(&1 in colors))

  defp add_rows(decks, rows, mapper) do
    room = max(limits().max_decks - length(decks), 0)
    accepted = Enum.take(rows, room)
    {Enum.reverse(Enum.map(accepted, mapper), decks), length(accepted) < length(rows)}
  end

  defp continue_pages(decks, deck_limit?, page, more?, next_page) do
    cond do
      not more? -> {:ok, Enum.reverse(decks)}
      deck_limit? or length(decks) >= limits().max_decks -> truncated(decks, "deck")
      page >= limits().max_pages -> truncated(decks, "page")
      true -> next_page.()
    end
  end

  defp budget_truncated(decks, reason),
    do: {:truncated, Enum.reverse(decks), budget_message(reason)}

  defp budget_message(:byte_limit),
    do: "Remote listing was truncated after reaching the 2 MB response budget."

  defp budget_message(:duration_limit),
    do: "Remote listing was truncated after reaching the 15 second time budget."

  defp truncated(decks, limit) do
    {:truncated, Enum.reverse(decks), "Remote listing was truncated at the #{limit} limit."}
  end

  defp limits do
    Map.merge(@default_limits, Application.get_env(:the_gathering, :remote_decks_limits, %{}))
  end

  defp settings_fingerprint(user) do
    [
      user.moxfield_username,
      user.archidekt_username,
      user.manavault_url,
      user.manavault_api_key
    ]
    |> :erlang.term_to_binary()
    |> then(&:crypto.hash(:sha256, &1))
  end

  defp sort_decks(decks) do
    Enum.sort_by(decks, &{&1.updated_at || "", &1.name || ""}, :desc)
  end
end
