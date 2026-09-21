defmodule TheGathering.Games.WinCondition do
  @moduledoc "Canonical game win conditions and Mythic Track's persisted numeric mapping."

  @values [
    {"damage", "Damage"},
    {"infinite_combo", "Infinite Combo"},
    {"mill", "Mill"},
    {"poison", "Poison"},
    {"alternate_win_con", "On-card Alternate Win Con"},
    {"hard_lock", "Hard Lock"},
    {"commander_damage", "Commander Damage"},
    {"draw", "Draw"},
    {"non_combat_damage", "Non-Combat Damage"},
    {"combat_damage", "Combat Damage"},
    {"concede", "Concede"},
    {"unknown", "Unknown"}
  ]

  @mythic_mapping %{
    1 => "damage",
    2 => "infinite_combo",
    3 => "mill",
    4 => "poison",
    5 => "alternate_win_con",
    6 => "hard_lock",
    7 => "commander_damage",
    8 => "draw",
    9 => "non_combat_damage",
    10 => "combat_damage",
    11 => "concede",
    99 => "unknown"
  }

  def values, do: @values
  def keys, do: Enum.map(@values, &elem(&1, 0))

  def label(key),
    do: @values |> Enum.find({"unknown", "Unknown"}, &(elem(&1, 0) == key)) |> elem(1)

  def from_mythic(value), do: Map.get(@mythic_mapping, value, "unknown")
end
