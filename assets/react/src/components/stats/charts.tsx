import type { ReactNode } from "react"
import { Link, type LinkProps } from "@tanstack/react-router"
import { linePoints, type NamedRecordRow, type TrendPoint } from "@/lib/stats"
import { CardArtBackground } from "@/components/card-art-background"
import { cn } from "@/lib/cn"

export function BarChart({
  rows,
  value = "win_rate",
  renderLabel,
  linkTo,
  columns = 1,
}: {
  rows: NamedRecordRow[]
  value?: "win_rate" | "games"
  renderLabel?: (row: NamedRecordRow) => ReactNode
  /** When given, each row becomes a link to the returned route options. */
  linkTo?: (row: NamedRecordRow) => LinkProps
  columns?: 1 | 2
}) {
  const max = Math.max(...rows.map((row) => row[value]), 1)
  return (
    <div
      className={cn(
        "grid grid-cols-1 gap-3",
        columns === 2 && "md:grid-flow-col md:grid-cols-2 md:gap-x-8",
      )}
      style={
        columns === 2
          ? { gridTemplateRows: `repeat(${Math.max(1, Math.ceil(rows.length / 2))}, auto)` }
          : undefined
      }
    >
      {rows.map((row) => {
        const link = linkTo?.(row)
        const className = cn(
          "block",
          row.art_crop_url && "relative overflow-hidden rounded-lg px-3 py-2",
          link && "hover:bg-base-300/60 -mx-1 rounded-lg px-1 transition-colors",
          link && row.art_crop_url && "hover:ring-primary/50 mx-0 px-3 hover:ring-1",
        )
        const content = (
          <>
            <CardArtBackground imageUrl={row.art_crop_url} interactive={Boolean(link)} />
            <div className="text-base-content relative z-10 mb-1 flex justify-between gap-3 text-sm">
              <span
                className={cn(
                  "truncate font-medium",
                  link && "group-hover:text-primary decoration-primary/60 group-hover:underline",
                )}
              >
                {renderLabel?.(row) ?? row.name}
              </span>
              <span className="text-base-content/80 tabular-nums">
                {value === "win_rate" ? `${row.win_rate}%` : row.games}
              </span>
            </div>
            <div className="bg-base-300 relative z-10 h-2 overflow-hidden rounded-full">
              <div
                className="bg-primary h-full rounded-full"
                style={{ width: `${(row[value] / max) * 100}%` }}
              />
            </div>
          </>
        )
        return link ? (
          <Link key={row.id} {...link} className={cn(className, "group")}>
            {content}
          </Link>
        ) : (
          <div key={row.id} className={className}>
            {content}
          </div>
        )
      })}
    </div>
  )
}

export function LineChart({ points, className }: { points: TrendPoint[]; className?: string }) {
  if (!points.length) return <p className="text-base-content/50 text-sm">No history yet.</p>
  return (
    <div>
      <svg
        viewBox="0 0 300 100"
        preserveAspectRatio="none"
        className={cn("h-28 w-full overflow-visible", className)}
        role="img"
        aria-label="Win rate over time"
      >
        <path
          d="M0 100H300 M0 50H300 M0 0H300"
          className="stroke-base-300"
          strokeWidth="1"
          vectorEffect="non-scaling-stroke"
        />
        <polyline
          points={linePoints(points.map((point) => point.win_rate))}
          fill="none"
          className="stroke-primary"
          strokeWidth="3"
          strokeLinecap="round"
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      <div className="text-base-content/50 flex justify-between text-xs">
        <span>{points.at(0)!.date}</span>
        <strong className="text-primary">{points.at(-1)?.win_rate}%</strong>
        <span>{points.at(-1)?.date}</span>
      </div>
    </div>
  )
}
