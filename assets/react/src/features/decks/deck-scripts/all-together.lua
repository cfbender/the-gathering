-- All together
--
-- Several rules, each a multiplier on the built-in weight, with every rule
-- that applied listed in the note. Remove a rule from RULES to turn it off.

local function quick_loss(deck)
  local last = deck.last_game
  if not last or not last.lost or last.days_ago >= 14 then
    return
  end
  local quick = (last.duration_minutes and last.duration_minutes < 45)
    or (last.turns and last.turns < 7)
  if quick then
    return 2.5, "quick loss"
  end
end

local function same_colors(deck, ctx)
  local last = ctx.last_game
  if last and last.deck_id and last.color_identity == deck.color_identity then
    return 0.5, "same colors"
  end
end

local function played_tonight(deck)
  if deck.plays_this_session > 0 then
    return 0.2, "played tonight"
  end
end

local function long_and_late(deck, ctx)
  local hour = ctx.now.hour
  local late = hour >= 23 or hour < 4
  if late and deck.avg_duration_minutes and deck.avg_duration_minutes > 90 then
    return 0.5, "long, late"
  end
end

local function brand_new(deck)
  if deck.never_played then
    return 2, "new"
  end
end

local RULES = {
  quick_loss,
  same_colors,
  played_tonight,
  long_and_late,
  brand_new,
}

function weight(deck, ctx)
  local w = deck.base_weight
  local reasons = {}
  for _, rule in ipairs(RULES) do
    local factor, reason = rule(deck, ctx)
    if factor then
      w = w * factor
      reasons[#reasons + 1] = string.format("%s x%g", reason, factor)
    end
  end
  if #reasons == 0 then
    return w
  end
  return w, table.concat(reasons, ", ")
end
