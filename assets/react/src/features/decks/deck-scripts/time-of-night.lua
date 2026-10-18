-- Time of night
--
-- Late at night, favor decks that play fast. On weekends, give new and barely
-- played decks a chance. On Fridays, bring a deck that wins. ctx.now is your
-- local time.

local LATE_HOUR = 22 -- 10 PM
local EARLY_HOUR = 4 -- until 4 AM still counts as late

local function late(now)
  return now.hour >= LATE_HOUR or now.hour < EARLY_HOUR
end

function weight(deck, ctx)
  local w = deck.base_weight

  if late(ctx.now) and deck.avg_duration_minutes then
    -- 60-minute decks keep their weight, 30-minute decks double, 120-minute
    -- decks halve.
    local minutes = deck.avg_duration_minutes
    local factor = math.min(math.max(60 / minutes, 0.25), 3)
    local note = string.format("Late: %.0f min avg (x%.2f)", minutes, factor)
    return w * factor, note
  end

  if ctx.now.weekend and deck.plays < 3 then
    return w * 2, "Weekend: try something new"
  end

  local friday = ctx.now.weekday_name == "friday"
  if friday and deck.win_rate and deck.win_rate >= 0.4 then
    return w * 1.5, string.format("Friday: %.0f%% wins", deck.win_rate * 100)
  end

  return w
end
