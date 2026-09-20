defmodule TheGathering.Games.ColorIdentity do
  @moduledoc """
  Canonical WUBRG ordering and the community names for color combinations
  (Ravnica guilds, Alara shards, Tarkir wedges, Commander 2016 four-color names).
  """

  @order ~w(W U B R G)

  @names %{
    "" => "Colorless",
    "W" => "Mono-White",
    "U" => "Mono-Blue",
    "B" => "Mono-Black",
    "R" => "Mono-Red",
    "G" => "Mono-Green",
    "WU" => "Azorius",
    "WB" => "Orzhov",
    "WR" => "Boros",
    "WG" => "Selesnya",
    "UB" => "Dimir",
    "UR" => "Izzet",
    "UG" => "Simic",
    "BR" => "Rakdos",
    "BG" => "Golgari",
    "RG" => "Gruul",
    "WUB" => "Esper",
    "WUR" => "Jeskai",
    "WUG" => "Bant",
    "WBR" => "Mardu",
    "WBG" => "Abzan",
    "WRG" => "Naya",
    "UBR" => "Grixis",
    "UBG" => "Sultai",
    "URG" => "Temur",
    "BRG" => "Jund",
    "WUBR" => "Yore",
    "WUBG" => "Witch",
    "WURG" => "Ink",
    "WBRG" => "Dune",
    "UBRG" => "Glint",
    "WUBRG" => "Five-Color"
  }

  @doc "Reorders identity letters into WUBRG order, dropping duplicates and unknown letters."
  def canonical(nil), do: ""

  def canonical(identity) when is_binary(identity) do
    letters = String.graphemes(identity)
    @order |> Enum.filter(&(&1 in letters)) |> Enum.join()
  end

  @doc "The common name for a color combination, falling back to the canonical letters."
  def name(identity) do
    canonical = canonical(identity)
    Map.get(@names, canonical, canonical)
  end
end
