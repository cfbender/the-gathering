import { useState, type ReactNode } from "react"
import { Trophy } from "lucide-react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { winConditionLabel } from "@/features/games/games"
import type { WinConditionStats } from "@/lib/stats"

export function PlayerWinConditions({
  wins,
  losses,
}: {
  wins: WinConditionStats
  losses: WinConditionStats
}) {
  const [perspective, setPerspective] = useState<"wins" | "losses">("wins")
  return (
    <WinConditions
      stats={perspective === "wins" ? wins : losses}
      perspective={perspective}
      action={
        <ToggleGroup
          type="single"
          value={perspective}
          aria-label="Win condition outcome"
          className="join shrink-0"
          onValueChange={(value) => {
            if (value === "wins" || value === "losses") setPerspective(value)
          }}
        >
          <ToggleGroupItem
            value="wins"
            className={`btn btn-xs join-item ${perspective === "wins" ? "btn-primary" : "btn-ghost"}`}
          >
            Wins
          </ToggleGroupItem>
          <ToggleGroupItem
            value="losses"
            className={`btn btn-xs join-item ${perspective === "losses" ? "btn-primary" : "btn-ghost"}`}
          >
            Losses
          </ToggleGroupItem>
        </ToggleGroup>
      }
    />
  )
}

export function WinConditions({
  stats,
  perspective,
  action,
}: {
  stats: WinConditionStats
  perspective?: "wins" | "losses"
  action?: ReactNode
}) {
  const top = stats.conditions[0]
  const favorites = stats.conditions.filter((row) => row.games === top?.games)
  const title =
    perspective === "wins"
      ? "Favorite win con"
      : perspective === "losses"
        ? "Win cons lost to"
        : "Win conditions"

  return (
    <section
      aria-label={title}
      className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-primary text-xs font-bold uppercase">How games end</p>
          <h2 className="text-xl font-bold">{title}</h2>
        </div>
        {action ?? <Trophy className="text-accent size-5 shrink-0" />}
      </div>
      {perspective && top && (
        <div className="mt-4">
          <p className="text-primary text-xl font-bold">
            {favorites.map((row) => winConditionLabel(row.condition)).join(" / ")}
          </p>
          <p className="text-base-content/60 mt-1 text-xs">
            {favorites.length > 1 ? "Tied for most frequent" : "Most frequent"} across their{" "}
            {perspective}
          </p>
        </div>
      )}
      <p className="text-base-content/60 mt-2 text-sm">
        Known for {stats.recorded_games} of {stats.total_games} {perspective ?? "games"}. Unknown
        and unrecorded excluded.
      </p>
      {top ? (
        <ul className="mt-5 space-y-3">
          {stats.conditions.map((row) => {
            const share = Math.round((row.games / stats.recorded_games) * 100)
            return (
              <li key={row.condition}>
                <div className="mb-1 flex items-baseline justify-between gap-3 text-sm">
                  <span className="font-medium">{winConditionLabel(row.condition)}</span>
                  <span className="text-base-content/70 shrink-0 tabular-nums">
                    {row.games} · {share}%
                  </span>
                </div>
                <div className="bg-base-300 h-2 overflow-hidden rounded-full" aria-hidden="true">
                  <div className="bg-primary h-full rounded-full" style={{ width: `${share}%` }} />
                </div>
              </li>
            )
          })}
        </ul>
      ) : (
        <p className="text-base-content/55 mt-5 text-sm">
          {perspective
            ? `No known win conditions for this player's ${perspective} yet.`
            : "No known win conditions in this range yet."}
        </p>
      )}
    </section>
  )
}
