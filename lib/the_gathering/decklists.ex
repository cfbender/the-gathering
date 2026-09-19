defmodule TheGathering.Decklists do
  @moduledoc "Parses deck-list links and resolves public deck metadata."

  alias TheGathering.Decklists.Cache
  alias TheGathering.Decklists.Sources.{Archidekt, Manavault, Moxfield}

  @sources %{
    moxfield: Moxfield,
    archidekt: Archidekt,
    manavault: Manavault
  }

  def parse_url(url) when is_binary(url) do
    with %URI{scheme: scheme, host: host} = uri
         when scheme in ["http", "https"] and is_binary(host) <-
           URI.parse(String.trim(url)),
         true <- valid_host?(host) do
      parse_uri(uri, String.downcase(host))
    else
      _ -> {:error, :invalid_url}
    end
  end

  def parse_url(_url), do: {:error, :invalid_url}

  def resolve(url) do
    with {:ok, %{source: source} = parsed} <- parse_url(url),
         {:ok, adapter} <- adapter(source) do
      case Cache.fetch(parsed.canonical_url) do
        {:ok, decklist} -> {:ok, decklist}
        :miss -> resolve_and_cache(adapter, parsed)
      end
    end
  end

  defp parse_uri(uri, host) when host in ["moxfield.com", "www.moxfield.com"] do
    case path_segments(uri.path) do
      ["decks", id | _] when id != "" ->
        {:ok, %{source: :moxfield, id: id, canonical_url: "https://moxfield.com/decks/#{id}"}}

      _ ->
        other(uri)
    end
  end

  defp parse_uri(uri, host) when host in ["archidekt.com", "www.archidekt.com"] do
    case path_segments(uri.path) do
      ["decks", id | _] ->
        if Regex.match?(~r/^\d+$/, id) do
          {:ok, %{source: :archidekt, id: id, canonical_url: "https://archidekt.com/decks/#{id}"}}
        else
          other(uri)
        end

      _ ->
        other(uri)
    end
  end

  defp parse_uri(uri, host) do
    case manavault_url() do
      %URI{host: manavault_host} = manavault
      when host == manavault_host or host == "www." <> manavault_host ->
        parse_manavault_uri(uri, manavault)

      _ ->
        other(uri)
    end
  end

  defp parse_manavault_uri(uri, manavault) do
    case path_segments(uri.path) do
      ["share", "decks", id | _] when byte_size(id) >= 20 ->
        {:ok,
         %{
           source: :manavault,
           id: id,
           canonical_url: "#{manavault}/share/decks/#{id}"
         }}

      _ ->
        other(uri)
    end
  end

  @doc """
  The configured self-hosted ManaVault origin (`MANAVAULT_URL`) as a `URI`, or `nil` when
  ManaVault links are not enabled. Only this origin is ever fetched, to avoid SSRF.
  """
  def manavault_url do
    case Application.get_env(:the_gathering, __MODULE__, [])[:manavault_url] do
      url when is_binary(url) and url != "" ->
        uri = URI.parse(String.trim_trailing(url, "/"))
        if is_binary(uri.host), do: %{uri | host: String.downcase(uri.host)}, else: nil

      _ ->
        nil
    end
  end

  defp other(uri) do
    canonical_url = URI.to_string(%{uri | fragment: nil})
    {:ok, %{source: :other, id: canonical_url, canonical_url: canonical_url}}
  end

  defp adapter(:other), do: {:error, :unsupported_url}
  defp adapter(source), do: Map.fetch(@sources, source)

  defp resolve_and_cache(adapter, parsed) do
    case adapter.resolve(parsed) do
      {:ok, decklist} = result ->
        :ok = Cache.put(parsed.canonical_url, decklist)
        result

      error ->
        error
    end
  end

  defp path_segments(path), do: path |> to_string() |> String.split("/", trim: true)

  defp valid_host?(host) do
    String.contains?(host, ".") or host in ["localhost", "::1"]
  end
end
