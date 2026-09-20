import { useState } from "react"
import { ColorIdentity } from "@/components/mana-symbols"
import { BarChart } from "@/components/stats/charts"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
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
}: {
  rows: NamedRecordRow[]
  eyebrow?: string
  minGames?: number
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
              className={cn("btn btn-xs join-item", metric === value ? "btn-primary" : "btn-ghost")}
            >
              {colorMetricLabels[value]}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
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
