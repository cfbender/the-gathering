import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { Gauge, Timer } from "lucide-react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import type { GameLengths as GameLengthsData, HistogramBin, RecentStatGame } from "@/lib/stats"

type LengthMetric = "duration" | "turns"

export function GameLengths({
  gameLengths,
  averageDuration,
  averageTurns,
  since,
}: {
  gameLengths: GameLengthsData
  averageDuration: number | null
  averageTurns: number | null
  since?: string
}) {
  const [metric, setMetric] = useState<LengthMetric>("duration")
  const bins = metric === "duration" ? gameLengths.durations : gameLengths.turns
  const average = metric === "duration" ? averageDuration : averageTurns

  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-primary text-xs font-bold uppercase">Pace of play</p>
          <h2 className="text-xl font-bold">
            Game lengths
            {since && (
              <span className="text-base-content/50 ml-2 text-sm font-medium">{since}</span>
            )}
          </h2>
        </div>
        <ToggleGroup
          type="single"
          value={metric}
          onValueChange={(value) => value && setMetric(value as LengthMetric)}
          aria-label="Game length metric"
          className="join"
        >
          {(["duration", "turns"] as const).map((value) => (
            <ToggleGroupItem
              key={value}
              value={value}
              className={cn("btn btn-xs join-item", metric === value ? "btn-primary" : "btn-ghost")}
            >
              {value === "duration" ? "Duration" : "Turns"}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      {bins.length === 0 ? (
        <p className="text-base-content/50 my-8 text-center text-sm">
          No {metric === "duration" ? "timed games" : "turn counts"} recorded yet.
        </p>
      ) : (
        <Histogram bins={bins} average={average} metric={metric} />
      )}

      <div className="mt-4 grid gap-3 sm:grid-cols-2">
        <GameCallout label="Fastest win" game={gameLengths.fastest_win} icon="fast" />
        <GameCallout label="Longest game" game={gameLengths.longest_game} icon="long" />
      </div>
    </section>
  )
}

function Histogram({
  bins,
  average,
  metric,
}: {
  bins: HistogramBin[]
  average: number | null
  metric: LengthMetric
}) {
  const maxGames = Math.max(...bins.map((bin) => bin.games), 1)
  const first = bins[0]!.from
  const last = bins.at(-1)!.to
  const averageX = average === null ? null : ((average - first) / (last - first)) * 100
  const labelEvery = Math.ceil(bins.length / 10)
  const columns = { gridTemplateColumns: `repeat(${bins.length}, minmax(0, 1fr))` }

  return (
    <div className="mt-5" role="img" aria-label={`${metric} histogram`}>
      {average !== null && (
        <p className="text-accent text-right text-xs font-bold">AVG {average}</p>
      )}
      <div className="overflow-x-auto">
        <div style={{ minWidth: `${Math.max(30, bins.length * 3.25)}rem` }}>
          <div className="border-base-content/20 relative border-b pt-6">
            <div className="grid h-36 items-end" style={columns}>
              {bins.map((bin) => (
                <div
                  key={bin.from}
                  title={`${bin.from}–${bin.to - 1}: ${bin.games} games`}
                  className="bg-primary/75 relative mx-1 rounded-t"
                  style={{ height: `${(bin.games / maxGames) * 100}%` }}
                >
                  {bin.games > 0 && (
                    <span className="text-base-content/65 absolute bottom-full mb-1 w-full text-center text-[10px]">
                      {bin.games}
                    </span>
                  )}
                </div>
              ))}
            </div>
            {averageX !== null && averageX >= 0 && averageX <= 100 && (
              <div
                className="border-accent pointer-events-none absolute top-6 bottom-0 border-l-2 border-dashed"
                style={{ left: `${averageX}%` }}
              />
            )}
          </div>
          <div className="text-base-content/55 grid pt-2 text-center text-[10px]" style={columns}>
            {bins.map((bin, index) => (
              <span key={bin.from}>
                {index % labelEvery === 0 ? `${bin.from}–${bin.to - 1}` : ""}
              </span>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}

function GameCallout({
  label,
  game,
  icon,
}: {
  label: string
  game: RecentStatGame | null
  icon: "fast" | "long"
}) {
  if (!game) return null
  const Icon = icon === "fast" ? Gauge : Timer
  return (
    <Link
      to="/games/$gameId"
      params={{ gameId: String(game.id) }}
      className="border-base-300 hover:border-primary/40 flex items-center gap-3 rounded-lg border p-3 transition-colors"
    >
      <span className="bg-primary/10 text-primary grid size-9 shrink-0 place-items-center rounded-lg">
        <Icon className="size-4" />
      </span>
      <span className="min-w-0">
        <small className="text-base-content/50 block font-bold uppercase">{label}</small>
        <strong className="block truncate">
          {game.winner?.name ?? "Draw game"} · {game.duration_minutes} min
        </strong>
      </span>
    </Link>
  )
}
