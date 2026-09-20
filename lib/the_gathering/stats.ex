defmodule TheGathering.Stats do
  @moduledoc """
  Read-only statistics derived from games and their normalized seats.

  Every win/loss/draw figure uses all games. Figures built from data a playgroup
  may only have started recording later (seat positions, duration, turns, MVP
  cards) use only games played on or after the administrator's
  `detailed_stats_from` date; each payload reports that date so the UI can label
  those figures.
  """

  alias TheGathering.Stats.{Commanders, Deck, Overview, Player}

  defdelegate overview(params \\ %{}), to: Overview, as: :get
  defdelegate player(player_id, params \\ %{}), to: Player, as: :get
  defdelegate deck(deck_id, params \\ %{}), to: Deck, as: :get

  @doc "Every commander played across the playgroup, most played first."
  defdelegate commanders(params \\ %{}), to: Commanders, as: :list

  @doc "Aggregate detail for one commander by Scryfall ID or card name."
  defdelegate commander(id, params \\ %{}), to: Commanders, as: :get
end
