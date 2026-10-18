-- Revenge
--
-- If someone beat you last game, favor decks with a good record against them.
-- Decks that have never faced them get a small bump to find out.

function weight(deck, ctx)
  local w = deck.base_weight
  local last = ctx.last_game
  if not last or last.won or #last.winners == 0 then
    return w
  end

  local rival = last.winners[1]
  local wins, games = 0, 0
  for _, game in ipairs(deck.games) do
    for _, opponent in ipairs(game.opponents) do
      if opponent.name == rival then
        games = games + 1
        if game.won then
          wins = wins + 1
        end
      end
    end
  end

  if games == 0 then
    return w * 1.25, "Untested against " .. rival
  end
  local rate = wins / games
  local note = string.format("vs %s: %d-%d", rival, wins, games - wins)
  return w * (0.5 + rate * 2.5), note
end
