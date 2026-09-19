defmodule TheGathering.Catalog.Scryfall do
  @moduledoc false

  @bulk_url "https://api.scryfall.com/bulk-data"
  @user_agent "TheGathering/0.1 (+https://github.com/cfbender/the-gathering)"

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
