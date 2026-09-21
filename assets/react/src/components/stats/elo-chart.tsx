import { buildEloPath, eloChartBounds, ratingToY, type EloSeries } from "@/lib/elo"

const WIDTH = 600
const HEIGHT = 210
const PADDING_X = 10
const PADDING_Y = 10
const PLOT_WIDTH = WIDTH - PADDING_X * 2
const PLOT_HEIGHT = HEIGHT - PADDING_Y * 2
const COLORS = [
  "var(--color-primary)",
  "var(--color-secondary)",
  "var(--color-accent)",
  "var(--color-info)",
  "var(--color-error)",
  "var(--color-base-content)",
]

export function EloChart({
  series,
  showLegend = true,
}: {
  series: EloSeries[]
  showLegend?: boolean
}) {
  const bounds = eloChartBounds(series)
  if (!bounds) return <p className="text-base-content/50 text-sm">No rating history yet.</p>

  const baselineY = PADDING_Y + ratingToY(1000, bounds.minRating, bounds.maxRating, PLOT_HEIGHT)

  // The SVG stretches to whatever box it is given (the section grid may make it
  // taller than its 600x210 viewBox), so strokes are drawn in screen space and
  // the baseline label lives outside the SVG to avoid distortion.
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="relative min-h-48 flex-1">
        <svg
          viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
          preserveAspectRatio="none"
          className="absolute inset-0 h-full w-full overflow-visible"
          role="img"
          aria-label="Elo rating over time"
        >
          <path
            d={`M${PADDING_X} ${baselineY}H${WIDTH - PADDING_X}`}
            className="stroke-base-content/25"
            strokeWidth="1"
            strokeDasharray="5 5"
            vectorEffect="non-scaling-stroke"
          />
          {series.map((player, index) => (
            <polyline
              key={player.id}
              points={buildEloPath(player.history, bounds, PLOT_WIDTH, PLOT_HEIGHT)}
              transform={`translate(${PADDING_X} ${PADDING_Y})`}
              fill="none"
              stroke={COLORS[index % COLORS.length]}
              strokeWidth="3"
              strokeLinecap="round"
              strokeLinejoin="round"
              vectorEffect="non-scaling-stroke"
            />
          ))}
        </svg>
        <span
          className="text-base-content/45 pointer-events-none absolute right-3 -translate-y-full text-[10px] leading-none"
          style={{ top: `${(baselineY / HEIGHT) * 100}%` }}
          aria-hidden="true"
        >
          1000
        </span>
      </div>
      <div className="text-base-content/50 flex justify-between text-xs">
        <span>{bounds.startDate}</span>
        <span>{bounds.endDate}</span>
      </div>
      {showLegend && (
        <ul className="mt-4 grid gap-x-4 gap-y-2 sm:grid-cols-2 xl:grid-cols-3">
          {series.map((player, index) => {
            const change = player.rating - 1000
            return (
              <li key={player.id} className="flex min-w-0 items-center gap-2 text-sm">
                <span
                  className="size-2.5 shrink-0 rounded-full"
                  style={{ backgroundColor: COLORS[index % COLORS.length] }}
                />
                <span className="truncate font-medium">{player.name}</span>
                <span className="text-base-content/55 ml-auto shrink-0 tabular-nums">
                  {player.rating} ({change >= 0 ? "+" : ""}
                  {change})
                </span>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
