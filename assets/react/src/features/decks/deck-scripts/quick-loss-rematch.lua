-- Quick-loss rematch
--
-- Gives a deck another shot when its last game was a fast loss, so one stomp
-- does not bury it. "Fast" uses the logged duration, or the turn count when
-- no duration was logged. The boost is strongest right after the loss and
-- fades away over two weeks.

local FAST_MINUTES = 45 -- a loss shorter than this is quick
local FAST_TURNS = 7 -- used when the game has no duration
local BOOST = 3 -- multiplier right after a quick loss
local MEMORY_DAYS = 14 -- the boost fades to nothing over this many days

local function quick_loss(game)
  if not game or not game.lost then
    return false
  end
  if game.duration_minutes then
    return game.duration_minutes < FAST_MINUTES
  end
  if game.turns then
    return game.turns < FAST_TURNS
  end
  return false
end

function weight(deck, ctx)
  local w = deck.base_weight
  local last = deck.last_game

  if quick_loss(last) and last.days_ago < MEMORY_DAYS then
    local fade = 1 - last.days_ago / MEMORY_DAYS
    local boost = 1 + (BOOST - 1) * fade
    local length = last.duration_minutes and (last.duration_minutes .. " min")
      or (last.turns .. " turns")
    local note = string.format("Rematch: lost in %s (x%.1f)", length, boost)
    return w * boost, note
  end

  return w
end
