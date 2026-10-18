import allTogether from "./deck-scripts/all-together.lua?raw"
import balanceWinRates from "./deck-scripts/balance-win-rates.lua?raw"
import colorVariety from "./deck-scripts/color-variety.lua?raw"
import starter from "./deck-scripts/default.lua?raw"
import newAndNeglected from "./deck-scripts/new-and-neglected.lua?raw"
import quickLossRematch from "./deck-scripts/quick-loss-rematch.lua?raw"
import revenge from "./deck-scripts/revenge.lua?raw"
import sessionPacing from "./deck-scripts/session-pacing.lua?raw"
import timeOfNight from "./deck-scripts/time-of-night.lua?raw"
import yourOwnFormula from "./deck-scripts/your-own-formula.lua?raw"

export interface DeckScriptExample {
  title: string
  description: string
  source: string
}

/** The script the editor starts from: the built-in weights with comments to build on. */
export const STARTER_SCRIPT = starter

/** The editor's Examples menu; `docs/deck-chooser-scripts.md` lists the same scripts. */
export const DECK_SCRIPT_EXAMPLES: DeckScriptExample[] = [
  {
    title: "Starter",
    description: "The built-in weights, with comments to build on.",
    source: starter,
  },
  {
    title: "Quick-loss rematch",
    description: "Boosts a deck whose last game was a fast loss, fading over two weeks.",
    source: quickLossRematch,
  },
  {
    title: "Color variety",
    description: "Favors colors you have not played in your last 10 games.",
    source: colorVariety,
  },
  {
    title: "Time of night",
    description: "Faster decks late at night, new decks on weekends, winners on Fridays.",
    source: timeOfNight,
  },
  {
    title: "Session pacing",
    description: "Open strong, avoid repeats, and favor shorter games later in the night.",
    source: sessionPacing,
  },
  {
    title: "Revenge",
    description: "If someone beat you last game, favor decks that have beaten them.",
    source: revenge,
  },
  {
    title: "New and neglected",
    description: "Boosts barely played decks and decks unplayed this month.",
    source: newAndNeglected,
  },
  {
    title: "Balance win rates",
    description: "More table time for struggling decks, less for winning streaks.",
    source: balanceWinRates,
  },
  {
    title: "Your own formula",
    description: "Ignores the built-in weight and starts from scratch.",
    source: yourOwnFormula,
  },
  {
    title: "All together",
    description: "Several rules as multipliers, each explained in the note.",
    source: allTogether,
  },
]
