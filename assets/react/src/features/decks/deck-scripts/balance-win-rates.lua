-- Balance win rates
--
-- Gives struggling decks more table time and rests decks that keep winning,
-- compared with your overall win rate. Decks with fewer than three games keep
-- their weight.

function weight(deck, ctx)
  local w = deck.base_weight
  local overall = ctx.stats.win_rate
  local games = deck.wins + deck.losses + deck.draws
  if not overall or not deck.win_rate or games < 3 then
    return w
  end

  -- Every 10 points below your average adds 25%; every 10 points above
  -- divides by 1.25.
  local gap = (overall - deck.win_rate) * 10
  local factor = gap >= 0 and 1 + gap * 0.25 or 1 / (1 - gap * 0.25)
  local note = string.format(
    "%.0f%% vs %.0f%% overall",
    deck.win_rate * 100,
    overall * 100
  )

  local streak = deck.streak
  if streak and streak.result == "win" and streak.length >= 2 then
    factor = factor * 0.5
    note = note .. string.format(", %d-win streak", streak.length)
  end

  return w * factor, note
end
