defmodule TheGathering.Decklists.RemoteDecks do
  @moduledoc "Lists and normalizes public decks from a user's configured deck hosts."

  alias TheGathering.Accounts.User
  alias TheGathering.Decklists.{Cache, HTTP}

  @moxfield_url "https://api2.moxfield.com/v2/decks/search-sfw"
  @archidekt_url "https://archidekt.com/api/decks/v3/"
  @colors ~w(W U B R G)

  def list(%User{} = user) do
    key =
      {:remote_decks, user.id, user.moxfield_username, user.archidekt_username,
       user.manavault_url}

    case Cache.fetch(key) do
      {:ok, result} -> result
      :miss -> fetch_and_cache(key, user)
    end
  end

  defp fetch_and_cache(key, user) do
    sources = [
      fetch_source(:moxfield, user.moxfield_username, &fetch_moxfield/1),
      fetch_source(:archidekt, user.archidekt_username, &fetch_archidekt/1),
      manavault_source(user.manavault_url)
    ]

    result = %{
      decks: sources |> Enum.flat_map(& &1.decks) |> sort_decks(),
      sources: Enum.map(sources, &Map.delete(&1, :decks))
    }

    :ok = Cache.put(key, result)
    result
  end

  defp fetch_source(source, value, _fetcher) when value in [nil, ""] do
    %{source: source, configured: false, error: nil, decks: []}
  end

  defp fetch_source(source, value, fetcher) do
    case fetcher.(value) do
      {:ok, decks} -> %{source: source, configured: true, error: nil, decks: decks}
      {:error, message} -> %{source: source, configured: true, error: message, decks: []}
    end
  end

  defp manavault_source(value) when value in [nil, ""] do
    %{source: :manavault, configured: false, error: nil, decks: []}
  end

  defp manavault_source(_url) do
    %{
      source: :manavault,
      configured: true,
      error:
        "ManaVault does not expose a public instance deck list. Add public share links directly when logging a game.",
      decks: []
    }
  end

  defp fetch_moxfield(username), do: fetch_moxfield_page(username, 1, [])

  defp fetch_moxfield_page(username, page, decks) do
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

    case HTTP.get("#{@moxfield_url}?#{query}") do
      {:ok, %Req.Response{status: 200, body: %{"data" => rows} = body}} when is_list(rows) ->
        normalized = decks ++ Enum.map(rows, &moxfield_deck/1)
        total_pages = body["totalPages"] || page

        if page < total_pages and rows != [] do
          fetch_moxfield_page(username, page + 1, normalized)
        else
          {:ok, normalized}
        end

      {:ok, %Req.Response{status: 404}} ->
        {:error, "Moxfield user was not found."}

      {:ok, %Req.Response{status: status}} when status in [401, 403, 429] ->
        {:error, "Moxfield blocked the request. Try again later or open decks on Moxfield."}

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

  defp fetch_archidekt(username) do
    query =
      URI.encode_query(%{"ownerUsername" => username, "page" => 1, "orderBy" => "-updatedAt"})

    with {:ok, rows} <- fetch_archidekt_pages("#{@archidekt_url}?#{query}", [], MapSet.new()) do
      fetch_archidekt_details(rows)
    end
  end

  defp fetch_archidekt_pages(url, decks, seen) do
    if MapSet.member?(seen, url) do
      {:error, "Archidekt returned invalid pagination."}
    else
      fetch_archidekt_page(url, decks, seen)
    end
  end

  defp fetch_archidekt_page(url, decks, seen) do
    case HTTP.get(url) do
      {:ok, %Req.Response{status: 200, body: %{"results" => rows} = body}} when is_list(rows) ->
        continue_archidekt_pages(body["next"], rows, url, decks, seen)

      {:ok, %Req.Response{status: 404}} ->
        {:error, "Archidekt user was not found."}

      {:ok, %Req.Response{status: status}} when status in [401, 403, 429] ->
        {:error, "Archidekt blocked the request. Try again later."}

      _ ->
        {:error, "Archidekt could not be reached. Try again shortly."}
    end
  end

  defp continue_archidekt_pages(next_value, rows, url, decks, seen) do
    case archidekt_next_url(next_value) do
      nil -> {:ok, decks ++ rows}
      next -> fetch_archidekt_pages(next, decks ++ rows, MapSet.put(seen, url))
    end
  end

  defp fetch_archidekt_details(rows) do
    results =
      Task.async_stream(rows, &fetch_archidekt_detail/1,
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

  defp fetch_archidekt_detail(row) do
    id = row["id"]

    case HTTP.get("https://archidekt.com/api/decks/#{id}/") do
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

  defp sort_decks(decks) do
    Enum.sort_by(decks, &{&1.updated_at || "", &1.name || ""}, :desc)
  end
end
