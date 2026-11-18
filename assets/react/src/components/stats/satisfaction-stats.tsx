import { Link } from "@tanstack/react-router"
import { cn } from "@/lib/cn"
import type { SatisfactionSummary, SeatSatisfaction } from "@/lib/stats"
import {
  SATISFACTION_LEVELS,
  SatisfactionBadge,
  SatisfactionFace,
} from "@/features/satisfaction/satisfaction"

/** One column per face, worst to best, sized by its share of the ratings. */
export function SatisfactionDistribution({
  summary,
  className,
}: {
  summary: SatisfactionSummary
  className?: string
}) {
  const most = Math.max(1, ...summary.counts)
  return (
    <ol className={cn("grid grid-cols-5 items-end gap-2", className)} aria-label="Ratings">
      {SATISFACTION_LEVELS.map((level, index) => {
        const count = summary.counts[index] ?? 0
        return (
          <li
            key={level.value}
            className="flex flex-col items-center gap-1"
            aria-label={`${level.label}: ${count}`}
          >
            <span className="text-base-content/70 text-xs tabular-nums">{count}</span>
            <div className="bg-base-300 flex h-20 w-full items-end overflow-hidden rounded-md">
              <div
                className={cn("w-full rounded-md bg-current opacity-70", level.color)}
                style={{ height: `${(count / most) * 100}%` }}
              />
            </div>
            <SatisfactionFace value={level.value} />
            <span className={cn("text-xs font-semibold", level.color)}>{level.label}</span>
          </li>
        )
      })}
    </ol>
  )
}

function Vibe({ label, summary }: { label: string; summary: SatisfactionSummary }) {
  return (
    <div>
      <p className="text-base-content/55 text-xs font-bold tracking-wider uppercase">{label}</p>
      <p className="mt-1 flex items-center gap-2 text-2xl font-black">
        {summary.average === null ? (
          "—"
        ) : (
          <SatisfactionBadge
            average
            value={summary.average}
            ratings={summary.ratings}
            className="[&_svg]:size-7 [&>span]:text-2xl"
          />
        )}
      </p>
      <p className="text-base-content/60 text-xs">
        {summary.ratings} {summary.ratings === 1 ? "rating" : "ratings"}
      </p>
    </div>
  )
}

/** A player's or deck's salt: how they rated their games and how their tables did. */
export function SatisfactionStats({
  satisfaction,
  own,
}: {
  satisfaction: SeatSatisfaction
  /** Label for the seat's own ratings, such as "Their vibe". */
  own: string
}) {
  return (
    <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <div className="mb-4 flex items-baseline justify-between gap-3">
        <h3 className="font-bold">Salt</h3>
        <Link to="/salt" className="link link-hover text-base-content/60 text-xs">
          Playgroup salt
        </Link>
      </div>
      {satisfaction.ratings === 0 && satisfaction.opponents.ratings === 0 ? (
        <p className="text-base-content/60 text-sm">
          No ratings yet. Players rate a game with one click when they are knocked out or the game
          ends.
        </p>
      ) : (
        <div className="grid grid-cols-1 gap-5 sm:grid-cols-[auto_minmax(0,1fr)] sm:items-center">
          <div className="flex gap-8 sm:flex-col sm:gap-4">
            <Vibe label={own} summary={satisfaction} />
            <Vibe label="Opponents' vibe" summary={satisfaction.opponents} />
          </div>
          <SatisfactionDistribution summary={satisfaction} />
        </div>
      )}
    </div>
  )
}
