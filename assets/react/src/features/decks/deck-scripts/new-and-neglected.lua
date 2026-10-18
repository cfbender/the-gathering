-- New and neglected
--
-- Pushes decks you have barely played: a new build gets a big boost that
-- fades over its first five games, and a deck untouched for a month gets a
-- nudge.

local NEW_GAMES = 5

function weight(deck, ctx)
  local w = deck.base_weight

  if deck.plays < NEW_GAMES then
    -- x3.5 when unplayed, x1.5 after four games.
    local boost = 1 + (NEW_GAMES - deck.plays) * 0.5
    return w * boost, string.format("New: %d plays (x%.1f)", deck.plays, boost)
  end

  if deck.plays_last_30_days == 0 then
    return w * 1.5, "Not played this month"
  end

  return w
end
