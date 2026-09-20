defmodule TheGatheringWeb.API.DeckChooserJSON do
  alias TheGatheringWeb.API.DeckJSON

  def show(%{pick: %{deck: nil, reason: reason}, card_art: _card_art}) do
    %{data: %{deck: nil, reason: reason}}
  end

  def show(%{pick: pick, card_art: card_art}) do
    %{
      data: %{
        deck: DeckJSON.summary(pick.deck, card_art),
        play_count: pick.play_count,
        skip_count: pick.deck.skip_count,
        last_played_at: pick.last_played_at,
        reason: nil
      }
    }
  end

  def outcome(%{deck: deck, outcome: outcome}) do
    %{data: %{deck_id: deck.id, outcome: outcome, skip_count: deck.skip_count}}
  end

  def sync(%{counts: counts}), do: %{data: counts}
end
