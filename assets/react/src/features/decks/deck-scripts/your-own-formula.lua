-- Your own formula
--
-- Ignores the built-in weight and starts from scratch: every deck starts
-- equal, time since it was last played adds weight with diminishing returns,
-- and each skip adds half again.

local NEVER_PLAYED_DAYS = 60 -- never-played decks count as this many days old

function weight(deck, ctx)
  local days = deck.days_since_played or NEVER_PLAYED_DAYS
  local w = 1 + math.sqrt(days)
  w = w * (1 + deck.skips * 0.5)
  return w, string.format("%.0f days, %d skips", days, deck.skips)
end
