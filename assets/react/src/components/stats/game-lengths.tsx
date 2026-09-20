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
  const chartWidth = Math.max(480, bins.length * 58)
  const chartHeight = 210
  const top = 24
  const bottom = 42
  const plotHeight = chartHeight - top - bottom
  const maxGames = Math.max(...bins.map((bin) => bin.games), 1)
  const first = bins[0]!.from
  const last = bins.at(-1)!.to
  const averageX = average === null ? null : ((average - first) / (last - first)) * chartWidth
  const labelEvery = Math.ceil(bins.length / 10)

  return (
    <div className="mt-5 overflow-x-auto" role="img" aria-label={`${metric} histogram`}>
      <svg
        viewBox={`0 0 ${chartWidth} ${chartHeight}`}
        width={chartWidth}
        className="h-52 min-w-full max-w-none"
      >
        <path
          d={`M0 ${top + plotHeight}H${chartWidth}`}
          className="stroke-base-content/20"
          strokeWidth="1"
        />
        {bins.map((bin, index) => {
          const slot = chartWidth / bins.length
          const height = (bin.games / maxGames) * plotHeight
          const x = index * slot + 4
          const y = top + plotHeight - height
          const label = `${bin.from}–${bin.to - 1}`
          return (
            <g key={bin.from}>
              <title>{`${label}: ${bin.games} games`}</title>
              <rect
                x={x}
                y={y}
                width={Math.max(slot - 8, 3)}
                height={height}
                rx="4"
                className="fill-primary/75"
              />
              {bin.games > 0 && (
                <text
                  x={x + (slot - 8) / 2}
                  y={y - 5}
                  textAnchor="middle"
                  className="fill-base-content/65 text-[10px]"
                >
                  {bin.games}
                </text>
              )}
              {index % labelEvery === 0 && (
                <text
                  x={index * slot + slot / 2}
                  y={chartHeight - 15}
                  textAnchor="middle"
                  className="fill-base-content/55 text-[10px]"
                >
                  {label}
                </text>
              )}
            </g>
          )
        })}
        {averageX !== null && averageX >= 0 && averageX <= chartWidth && (
          <g>
            <path
              d={`M${averageX} ${top - 4}V${top + plotHeight}`}
              className="stroke-accent"
              strokeWidth="3"
              strokeDasharray="5 4"
            />
            <text
              x={averageX < chartWidth - 80 ? averageX + 5 : averageX - 5}
              y={12}
              textAnchor={averageX < chartWidth - 80 ? "start" : "end"}
              className="fill-accent text-[10px] font-bold"
            >
              AVG {average}
            </text>
          </g>
        )}
      </svg>
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
