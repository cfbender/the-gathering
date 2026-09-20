defmodule TheGathering.Catalog.Sync do
  @moduledoc "Streams a Scryfall generation into staging and atomically publishes it."

  import Ecto.Query

  alias Ecto.Adapters.SQL
  alias TheGathering.Catalog
  alias TheGathering.Catalog.{Card, CardData, Gzip, Scryfall, StagedCard, SyncState}
  alias TheGathering.Repo

  @batch_size 250
  @card_columns ~w(id oracle_id name normalized_name mana_cost cmc type_line oracle_text colors color_identity image_uris set_code collector_number released_at layout rarity commander_legal can_be_commander commander_pairing inserted_at updated_at)

  def run(opts \\ []) do
    state = start_state()

    try do
      source = source(Keyword.get(opts, :source, :scryfall))
      Repo.delete_all(StagedCard)

      source.path
      |> stream(source.compressed?)
      |> Stream.map(&decode!/1)
      |> Stream.map(&CardData.from_scryfall/1)
      |> Stream.reject(&is_nil/1)
      |> Stream.chunk_every(@batch_size)
      |> Enum.each(&stage_batch/1)

      count = Repo.aggregate(StagedCard, :count)
      if count == 0, do: raise("staged catalog generation is empty")

      publish!()
      finish_state(state, count, source.updated_at)
      Catalog.backfill()
      {:ok, count}
    rescue
      error ->
        fail_state(state, Exception.format(:error, error, __STACKTRACE__))
        {:error, Exception.message(error)}
    after
      cleanup(Keyword.get(opts, :source, :scryfall))
    end
  end

  defp source(:scryfall) do
    result = Scryfall.fetch()
    Process.put({__MODULE__, :temporary_path}, result.path)
    Map.put(result, :compressed?, true)
  end

  defp source({:file, path}), do: %{path: path, updated_at: nil, compressed?: false}
  defp source({:gzip_file, path}), do: %{path: path, updated_at: nil, compressed?: true}

  defp cleanup(:scryfall) do
    if path = Process.delete({__MODULE__, :temporary_path}), do: File.rm(path)
  end

  defp cleanup(_source), do: :ok

  defp stream(path, true), do: Gzip.lines(path)
  defp stream(path, false), do: File.stream!(path, :line, [])

  defp decode!(line) do
    case Jason.decode(String.trim(line)) do
      {:ok, object} when is_map(object) -> object
      {:ok, _other} -> raise "Scryfall bulk line is not a JSON object"
      {:error, error} -> raise "invalid Scryfall bulk JSON: #{Exception.message(error)}"
    end
  end

  defp stage_batch(rows) do
    candidates =
      rows
      |> Enum.group_by(& &1.oracle_id)
      |> Map.new(fn {oracle_id, printings} ->
        {oracle_id, Enum.max_by(printings, & &1.selection_key)}
      end)

    existing =
      StagedCard
      |> where([card], card.oracle_id in ^Map.keys(candidates))
      |> select([card], {card.oracle_id, card.selection_key})
      |> Repo.all()
      |> Map.new()

    winners =
      candidates
      |> Enum.reject(fn {oracle_id, candidate} ->
        Map.get(existing, oracle_id, "") >= candidate.selection_key
      end)
      |> Enum.map(&elem(&1, 1))

    Repo.transaction(fn ->
      Repo.insert_all(StagedCard, winners,
        conflict_target: :oracle_id,
        on_conflict: {:replace_all_except, [:oracle_id, :inserted_at]}
      )
    end)
  end

  defp publish! do
    columns = Enum.join(@card_columns, ", ")

    Repo.transaction(fn ->
      Repo.delete_all(Card)

      SQL.query!(
        Repo,
        "INSERT INTO cards (#{columns}) SELECT #{columns} FROM catalog_cards_staging",
        []
      )
    end)
  end

  defp start_state do
    now = now()

    %SyncState{}
    |> Ecto.Changeset.change(%{
      status: "running",
      last_started_at: now,
      card_count: Repo.aggregate(Card, :count),
      last_error: nil
    })
    |> Repo.insert!()
  end

  defp finish_state(state, count, updated_at) do
    state
    |> Ecto.Changeset.change(%{
      status: "succeeded",
      last_finished_at: now(),
      card_count: count,
      scryfall_updated_at: updated_at,
      last_error: nil
    })
    |> Repo.update!()
  end

  defp fail_state(state, error) do
    state
    |> Ecto.Changeset.change(%{
      status: "failed",
      last_finished_at: now(),
      last_error: String.slice(error, 0, 4_000)
    })
    |> Repo.update!()
  end

  defp now, do: DateTime.utc_now() |> DateTime.truncate(:second)
end
