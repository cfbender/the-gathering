-- Color variety
--
-- Favors decks whose colors you have not played lately. ctx.recent_colors
-- counts each color across your last 10 games (C counts colorless decks), and
-- every recent game that shared a color makes a deck less likely. The exact
-- colors of your last game are halved again.

local PENALTY = 0.75 -- multiplier for each recent game that shared a color
local FLOOR = 0.1 -- never drop a deck below this share of its weight

function weight(deck, ctx)
  local keys = deck.colorless and { "C" } or deck.colors
  local factor = 1
  for _, color in ipairs(keys) do
    factor = factor * PENALTY ^ (ctx.recent_colors[color] or 0)
  end
  -- Average per color, so five-color decks are not punished just for having
  -- more colors.
  factor = math.max(factor ^ (1 / #keys), FLOOR)

  local last = ctx.last_game
  if last and last.deck_id and last.color_identity == deck.color_identity then
    factor = factor * 0.5
  end

  local note = string.format("%s x%.2f", deck.color_name, factor)
  return deck.base_weight * factor, note
end
