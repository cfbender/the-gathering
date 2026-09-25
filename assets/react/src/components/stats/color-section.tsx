import { useState, type ReactNode } from "react"
import { ColorIdentity } from "@/components/mana-symbols"
import { BarChart } from "@/components/stats/charts"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { gamesLink, type GamesLinkScope } from "@/features/games/game-filters"
import { cn } from "@/lib/cn"
import { identityFilterValue } from "@/lib/color-identities"
import {
  LEADERBOARD_MIN_GAMES,
  sortByMetric,
  type ColorMetric,
  type NamedRecordRow,
} from "@/lib/stats"

const colorMetricLabels: Record<ColorMetric, string> = {
  games: "Popularity",
  win_rate: "Win rate",
}

/**
 * Color-identity breakdown with a Popularity / Win rate toggle. The win-rate view
 * hides rows below `minGames` so a single lucky game cannot top the chart.
 */
export function ColorSection({
  rows,
  eyebrow = "Color check",
  minGames = LEADERBOARD_MIN_GAMES,
  action,
  games,
}: {
  rows: NamedRecordRow[]
  eyebrow?: string
  minGames?: number
  /** Rendered after the metric toggle, for example a "View all" link. */
  action?: ReactNode
  /** Links rows to their games: games played in the Popularity view, wins in Win rate. */
  games?: GamesLinkScope
}) {
  const [metric, setMetric] = useState<ColorMetric>("games")
  const shown = sortByMetric(rows, metric, minGames).slice(0, 6)
  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-primary text-xs font-bold uppercase">{eyebrow}</p>
          <h2 className="text-xl font-bold">
            {metric === "games" ? "Most played colors" : "Color win rates"}
          </h2>
        </div>
        <div className="flex items-center gap-2">
          <ToggleGroup
            type="single"
            value={metric}
            onValueChange={(value) => value && setMetric(value as ColorMetric)}
            aria-label="Color metric"
            className="join"
          >
            {(Object.keys(colorMetricLabels) as ColorMetric[]).map((value) => (
              <ToggleGroupItem
                key={value}
                value={value}
                className={cn(
                  "btn btn-xs join-item",
                  metric === value ? "btn-primary" : "btn-ghost",
                )}
              >
                {colorMetricLabels[value]}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          {action}
        </div>
      </div>
      {shown.length === 0 ? (
        <p className="text-base-content/50 text-sm">
          {rows.length === 0
            ? "No games with a deck recorded yet."
            : `Play at least ${minGames} games in a color to rank its win rate.`}
        </p>
      ) : (
        <BarChart
          rows={shown}
          value={metric}
          linkTo={games && ((row) => colorIdentityLink(games, metric, row))}
          renderLabel={(row) => (
            <span className="inline-flex items-center gap-2">
              <ColorIdentity colors={String(row.id)} />
              <span>{row.name}</span>
              <span className="text-base-content/50 text-xs font-normal">
                {metric === "win_rate"
                  ? `${row.games} ${row.games === 1 ? "game" : "games"}`
                  : `${row.win_rate}% win rate`}
              </span>
            </span>
          )}
        />
      )}
    </section>
  )
}

/** Games where the identity was played or, for win rates, won with; on a profile, by that player. */
export function colorIdentityLink(scope: GamesLinkScope, metric: ColorMetric, row: NamedRecordRow) {
  const colors = identityFilterValue(row.id)
  return metric === "games"
    ? gamesLink(scope, { player_id: scope.player_id, colors })
    : gamesLink(scope, { winner_id: scope.player_id, winner_colors: colors })
}
