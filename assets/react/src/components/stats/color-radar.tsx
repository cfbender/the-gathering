import { ManaSymbol } from "@/components/mana-symbols"
import type { ColorExposure } from "@/lib/stats"

const center = 120
const radius = 84

function point(index: number, percent: number) {
  const angle = -Math.PI / 2 + (index * Math.PI * 2) / 5
  const distance = radius * (percent / 100)
  return `${center + Math.cos(angle) * distance},${center + Math.sin(angle) * distance}`
}

function polygon(values: number[]) {
  return values.map((value, index) => point(index, value)).join(" ")
}

export function ColorRadar({ rows }: { rows: ColorExposure[] }) {
  if (rows.every((row) => row.games === 0)) {
    return (
      <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
        <p className="text-primary text-xs font-bold uppercase">Color profile</p>
        <h2 className="text-xl font-bold">Exposure and success</h2>
        <p className="text-base-content/50 mt-5 text-sm">No games with a deck recorded yet.</p>
      </section>
    )
  }

  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <p className="text-primary text-xs font-bold uppercase">Color profile</p>
      <h2 className="text-xl font-bold">Exposure and success</h2>
      <div className="mx-auto mt-2 max-w-sm">
        <svg
          viewBox="0 0 240 240"
          className="w-full"
          role="img"
          aria-label="Color exposure and win rate radar chart"
        >
          {[25, 50, 75, 100].map((percent) => (
            <polygon
              key={percent}
              points={polygon(Array(5).fill(percent))}
              fill="none"
              className="stroke-base-content/20"
              strokeWidth="1"
            />
          ))}
          {rows.map((row, index) => (
            <line
              key={row.id}
              x1={center}
              y1={center}
              x2={point(index, 100).split(",")[0]}
              y2={point(index, 100).split(",")[1]}
              className="stroke-base-content/15"
            />
          ))}
          <polygon
            points={polygon(rows.map((row) => row.share))}
            className="fill-primary/35 stroke-primary"
            strokeWidth="2.5"
          />
          <polygon
            points={polygon(rows.map((row) => row.win_rate))}
            className="fill-secondary/35 stroke-secondary"
            strokeWidth="2.5"
          />
          {rows.map((row, index) => {
            const [x = 0, y = 0] = point(index, 116).split(",").map(Number)
            return (
              <foreignObject key={row.id} x={x - 14} y={y - 14} width="28" height="28">
                <div className="flex h-full items-center justify-center text-xl">
                  <ManaSymbol symbol={row.id} className="m-0 translate-y-0" />
                </div>
              </foreignObject>
            )
          })}
        </svg>
        <div className="mb-4 flex flex-wrap justify-center gap-x-5 gap-y-1 text-xs font-medium">
          <span className="inline-flex items-center gap-1.5">
            <span className="bg-primary size-2.5 rounded-sm" /> Exposure
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="bg-secondary size-2.5 rounded-sm" /> Win rate
          </span>
        </div>
      </div>
      <div className="grid grid-cols-5 gap-1 text-center text-sm tabular-nums">
        {rows.map((row) => (
          <div key={row.id} title={`${row.name}: ${row.games} games`}>
            <ManaSymbol symbol={row.id} className="mx-auto mb-1 block translate-y-0" />
            <strong className="text-primary block">{row.share}%</strong>
            <span className="text-secondary block">{row.win_rate}%</span>
          </div>
        ))}
      </div>
    </section>
  )
}
