-- Deck chooser script
--
-- The chooser calls weight(deck, ctx) once for every deck it can pick, then
-- draws one at random in proportion to the numbers you return: a deck
-- weighted 20 is twice as likely as a deck weighted 10, and 0 leaves a deck
-- out. Return a second value to explain the weight; the chooser shows it on
-- the pick and the preview shows it beside each deck.
--
-- deck.base_weight is the built-in weight: hours since the deck was last
-- played, times (skips + 1), divided by (plays + 1). Adjust it, or ignore it
-- and write your own formula.
--
-- The Reference tab lists every field on deck and ctx. In the preview, click
-- a deck to see the exact table it receives, open the Context tab to see ctx,
-- and use print() to write to the log. Load an example from the Examples menu
-- to see the fields in use.

function weight(deck, ctx)
  local w = deck.base_weight

  -- A few ideas to uncomment and tune:
  -- if deck.never_played then w = w * 2 end
  -- if deck.last_game and deck.last_game.lost then w = w * 1.5 end
  -- if deck.plays_this_session > 0 then w = w * 0.25 end
  -- if ctx.now.weekend and deck.color_count >= 4 then w = w * 1.5 end

  return w
end
