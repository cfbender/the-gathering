-- Session pacing
--
-- Shapes a game night: open with a deck that has done well, avoid repeating
-- decks, and favor shorter games as the night goes on. ctx.session holds
-- tonight's games (games less than four hours apart), so a night that runs
-- past midnight still counts as one session, unlike ctx.games_today, which
-- resets at local midnight.

function weight(deck, ctx)
  local w = deck.base_weight
  local played = ctx.session_games

  if deck.plays_this_session > 0 then
    return w * 0.1 ^ deck.plays_this_session, "Already played tonight"
  end

  if played == 0 then
    local rate = deck.win_rate or 0.25 -- unproven decks count as average
    local note = string.format("Opener: %.0f%% wins", rate * 100)
    return w * (0.5 + rate * 2), note
  end

  if played >= 3 and deck.avg_turns then
    -- From the fourth game on, a 10-turn deck keeps its weight and shorter
    -- decks gain.
    local factor = math.min(math.max(10 / deck.avg_turns, 0.5), 2)
    local note =
      string.format("Game %d: avg %.1f turns", played + 1, deck.avg_turns)
    return w * factor, note
  end

  return w
end
