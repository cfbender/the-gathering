defmodule TheGathering.Imports.PortableFile do
  @moduledoc false
  alias TheGathering.Catalog.{Card, Printing}

  @collections ~w(players decks games cards printings sheet_receipts)
  @fields %{
    players: ~w(name archived_at)a,
    decks:
      ~w(name commander_card_id commander_name commander_printing_id partner_card_id partner_name partner_printing_id color_identity decklist_url archived_at skip_count included_for_play)a,
    games: ~w(portable_id played_at duration_minutes turns notes source external_id)a,
    seats:
      ~w(player_id deck_id seat result kills eliminated_turn eliminated_by_player_id mvp_card_id mvp_card_name notes)a
  }

  def fields(:cards), do: Card.__schema__(:fields) -- [:inserted_at, :updated_at]
  def fields(:printings), do: Printing.__schema__(:fields)
  def fields(kind), do: Map.fetch!(@fields, kind)
  def attrs(row, kind), do: Map.take(row, Enum.map(fields(kind), &Atom.to_string/1))

  def decode(json) when is_binary(json) do
    with {:ok, %{"format" => "the-gathering", "version" => 1} = data} <- Jason.decode(json),
         true <- Enum.all?(@collections, &records?(data[&1])),
         true <- unique_ids?(data["players"], "id", &local_id?/1),
         true <- unique_ids?(data["decks"], "id", &local_id?/1),
         true <- unique_ids?(data["games"], "portable_id", &uuid?/1),
         true <- Enum.all?(data["games"], &valid_game?/1) do
      {:ok, data}
    else
      _ ->
        {:error,
         "Choose a valid The Gathering JSON export (format version 1). Records need unique IDs and games need 2–6 seats."}
    end
  end

  def decode(_json), do: {:error, "Export contents must be JSON text."}

  defp records?(rows), do: is_list(rows) and Enum.all?(rows, &is_map/1)
  defp local_id?(id), do: is_integer(id) and id > 0
  defp uuid?(id), do: is_binary(id) and match?({:ok, ^id}, Ecto.UUID.cast(id))

  defp unique_ids?(rows, key, valid?) do
    ids = Enum.map(rows, & &1[key])
    Enum.all?(ids, valid?) and length(Enum.uniq(ids)) == length(ids)
  end

  defp valid_game?(game) do
    records?(game["seats"]) and length(game["seats"]) in 2..6 and
      game["source"] in ~w(manual csv mythic_track discord) and
      (is_nil(game["external_id"]) or is_binary(game["external_id"]))
  end
end
