import { Link } from "@tanstack/react-router"
import { ManaSymbol } from "@/components/mana-symbols"
import { gamesLink, type GamesLinkScope } from "@/features/games/game-filters"
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

/** With `games` (a profile scope), names link to games with the color and win rates to wins. */
export function ColorRadar({ rows, games }: { rows: ColorExposure[]; games?: GamesLinkScope }) {
  if (rows.every((row) => row.games === 0)) {
    return (
      <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
        <p className="text-primary text-xs font-bold uppercase">Color profile</p>
        <h2 className="text-xl font-bold">Played and won by color</h2>
        <p className="text-base-content/50 mt-5 text-sm">No games with a deck recorded yet.</p>
      </section>
    )
  }

  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <p className="text-primary text-xs font-bold uppercase">Color profile</p>
      <h2 className="text-xl font-bold">Played and won by color</h2>
      <p className="text-base-content/60 mt-1 text-sm">
        How often each color shows up in the decks they pilot, and how they fare when it does.
        Multicolor decks count toward every color they include.
      </p>
      <div className="mt-4 grid items-center gap-6 md:grid-cols-[minmax(0,20rem)_minmax(0,1fr)]">
        <div className="mx-auto w-full max-w-xs">
          <svg
            viewBox="0 0 240 240"
            className="w-full"
            role="img"
            aria-label="Share of games and win rate by color"
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
          <div className="flex flex-wrap justify-center gap-x-5 gap-y-1 text-xs font-medium">
            <span className="inline-flex items-center gap-1.5">
              <span className="bg-primary size-2.5 rounded-sm" /> % of games
            </span>
            <span className="inline-flex items-center gap-1.5">
              <span className="bg-secondary size-2.5 rounded-sm" /> Win rate
            </span>
          </div>
        </div>
        <table className="w-full text-sm tabular-nums">
          <thead className="text-base-content/50 text-xs font-bold uppercase">
            <tr>
              <th className="pb-2 text-left font-bold">Color</th>
              <th className="pb-2 text-right font-bold">Games</th>
              <th className="text-primary pb-2 text-right font-bold">% of games</th>
              <th className="text-secondary pb-2 text-right font-bold">Win rate</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id} className="border-base-300/70 border-t">
                <td className="py-2">
                  {games && row.games > 0 ? (
                    <Link
                      {...gamesLink(games, { player_id: games.player_id, color: row.id })}
                      className="hover:text-primary flex items-center gap-2 font-medium hover:underline"
                    >
                      <ManaSymbol symbol={row.id} className="m-0 translate-y-0" />
                      {row.name}
                    </Link>
                  ) : (
                    <span className="flex items-center gap-2 font-medium">
                      <ManaSymbol symbol={row.id} className="m-0 translate-y-0" />
                      {row.name}
                    </span>
                  )}
                </td>
                <td className="text-base-content/70 py-2 text-right">{row.games}</td>
                <td className="py-2 text-right font-semibold">{row.share}%</td>
                <td className="py-2 text-right font-semibold">
                  {games && row.wins > 0 ? (
                    <Link
                      {...gamesLink(games, { winner_id: games.player_id, winner_color: row.id })}
                      className="hover:text-primary hover:underline"
                      aria-label={`${row.win_rate}% win rate with ${row.name}. Show wins`}
                    >
                      {row.win_rate}%
                    </Link>
                  ) : (
                    `${row.win_rate}%`
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
