import { linePoints, type RecordStat, type TrendPoint } from "@/lib/stats"
import { CardArtBackground } from "@/components/card-art-background"

export function BarChart({
  rows,
  value = "win_rate",
}: {
  rows: RecordStat[]
  value?: "win_rate" | "games"
}) {
  const max = Math.max(...rows.map((row) => row[value]), 1)
  return (
    <div className="space-y-3">
      {rows.map((row) => (
        <div
          key={row.id}
          className={row.art_crop_url ? "relative overflow-hidden rounded-lg px-3 py-2" : undefined}
        >
          <CardArtBackground imageUrl={row.art_crop_url} />
          <div className="text-base-content relative z-10 mb-1 flex justify-between gap-3 text-sm">
            <span className="truncate font-medium">{row.name}</span>
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
        </div>
      ))}
    </div>
  )
}

export function LineChart({ points }: { points: TrendPoint[] }) {
  if (!points.length) return <p className="text-base-content/50 text-sm">No history yet.</p>
  return (
    <div>
      <svg
        viewBox="0 0 300 100"
        className="h-28 w-full overflow-visible"
        role="img"
        aria-label="Win rate over time"
      >
        <path d="M0 100H300 M0 50H300 M0 0H300" className="stroke-base-300" strokeWidth="1" />
        <polyline
          points={linePoints(points.map((point) => point.win_rate))}
          fill="none"
          className="stroke-primary"
          strokeWidth="4"
          strokeLinecap="round"
          strokeLinejoin="round"
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
