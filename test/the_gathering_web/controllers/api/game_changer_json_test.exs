defmodule TheGatheringWeb.API.GameChangerJSONTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.{Catalog, Repo}
  alias TheGathering.Catalog.{Card, CardData}
  alias TheGathering.Games.{Deck, GamePlayer}
  alias TheGatheringWeb.API.{DeckJSON, GameJSON}

  test "batched summaries label commanders, partners and MVPs using current catalog identity" do
    for {id, name, flag} <- [
          {"thrasios", "Thrasios, Triton Hero", true},
          {"kraum", "Kraum", false}
        ] do
      attrs =
        %{"id" => id, "oracle_id" => id, "name" => name, "game_changer" => flag}
        |> CardData.from_scryfall()
        |> Map.delete(:selection_key)

      Repo.insert!(struct!(Card, attrs))
    end

    refs = [{"thrasios", "Thrasios, Triton Hero"}, {"kraum", "Kraum"}]
    summaries = Catalog.card_summaries(refs)
    assert Catalog.card_summary(summaries, "thrasios", nil).game_changer
    art = Catalog.art_crop_urls(refs)
    # An obsolete ID falls back to the stored name; a known false ID must not.
    assert Catalog.game_changer?(art, "obsolete", "Thrasios, Triton Hero")
    refute Catalog.game_changer?(art, "kraum", "Thrasios, Triton Hero")
    refute Catalog.game_changer?(art, nil, "Missing")

    deck = %Deck{
      commander_card_id: "kraum",
      commander_name: "Kraum",
      partner_card_id: "obsolete",
      partner_name: "Thrasios, Triton Hero"
    }

    data = DeckJSON.summary(deck, art)
    refute data.commander_game_changer
    assert data.partner_game_changer
    seat = %GamePlayer{mvp_card_name: "Thrasios, Triton Hero"}
    assert GameJSON.seat(seat, art).mvp_game_changer
  end
end
