defmodule TheGathering.Imports.CSVChanges do
  @moduledoc false
  alias TheGathering.Games.WinCondition

  def diff(before, after_game) do
    fields = [:played_at, :duration_minutes, :turns, :win_condition, :notes]
    game_changes = changes(before, after_game, fields, nil)
    players = Enum.map(before.seats ++ after_game.seats, & &1.player_id) |> Enum.uniq()

    Enum.reduce(players, game_changes, fn id, diffs ->
      old = Enum.find(before.seats, &(&1.player_id == id))
      new = Enum.find(after_game.seats, &(&1.player_id == id))
      name = (old || new).player.name

      diffs ++
        changes(
          seat(old),
          seat(new),
          [:participant, :deck, :seat, :result, :kills, :mvp_card_name, :eliminated_by_player_id],
          name
        )
    end)
  end

  defp changes(before, after_value, fields, player) do
    Enum.flat_map(fields, fn field ->
      old = value(field, Map.get(before, field))
      new = value(field, Map.get(after_value, field))
      if old == new, do: [], else: [%{field: field, player: player, before: old, after: new}]
    end)
  end

  defp seat(nil), do: %{}

  defp seat(seat),
    do:
      seat
      |> Map.from_struct()
      |> Map.put(:participant, seat.player.name)
      |> Map.put(
        :deck,
        if(seat.deck,
          do:
            "#{seat.deck.name} (#{seat.deck.commander_name}#{if seat.deck.partner_name, do: " / " <> seat.deck.partner_name, else: ""})"
        )
      )

  defp value(:played_at, %DateTime{} = value), do: DateTime.to_iso8601(value)
  defp value(:win_condition, value) when is_binary(value), do: WinCondition.label(value)
  defp value(_field, value), do: value
end
