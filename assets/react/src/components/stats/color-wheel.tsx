import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { gamesLink, type GamesLinkScope } from "@/features/games/game-filters"
import { colorWheelSlices, donutSlicePath } from "@/lib/color-wheel"
import { cn } from "@/lib/cn"
import type { ColorExposure } from "@/lib/stats"

const colors: Record<ColorExposure["id"], string> = {
  W: "#f5f0d6",
  U: "#147cc1",
  B: "#2d2533",
  R: "#d84237",
  G: "#178653",
}

export function ColorWheel({
  rows,
  eyebrow = "Color spread",
  className,
  games,
}: {
  rows: ColorExposure[]
  eyebrow?: string
  className?: string
  /** Makes each slice a link to the games whose decks included that color. */
  games?: GamesLinkScope
}) {
  const [activeId, setActiveId] = useState<string | null>(null)
  const slices = colorWheelSlices(rows)
  const total = rows.reduce((sum, row) => sum + row.games, 0)
  const active = slices.find((slice) => slice.id === activeId)
  const activeRow = rows.find((row) => row.id === activeId)

  return (
    <section className={cn("border-base-300 bg-base-200/60 rounded-xl border p-5", className)}>
      <p className="text-primary text-xs font-bold uppercase">{eyebrow}</p>
      <h2 className="text-xl font-bold">Color identity wheel</h2>
      {total === 0 ? (
        <p className="text-base-content/50 mt-5 text-sm">No games with a deck recorded yet.</p>
      ) : (
        <svg
          viewBox="0 0 200 200"
          className="mx-auto mt-4 w-full max-w-64"
          role="img"
          aria-label="Color identity appearance share"
          onMouseLeave={() => setActiveId(null)}
        >
          {slices.map((slice) => {
            const label = `${rows.find((row) => row.id === slice.id)?.name ?? slice.id}: ${slice.percentage.toFixed(1)}%, ${slice.games} games`
            const path = (
              <path
                d={donutSlicePath(slice.startAngle, slice.endAngle)}
                fill={colors[slice.id as ColorExposure["id"]]}
                className={cn(
                  "stroke-base-content/35 cursor-pointer outline-none transition-opacity",
                  activeId !== null && activeId !== slice.id && "opacity-40",
                )}
                strokeWidth="2"
                tabIndex={games ? undefined : 0}
                aria-label={games ? undefined : label}
                onMouseEnter={() => setActiveId(slice.id)}
                onFocus={() => setActiveId(slice.id)}
                onBlur={() => setActiveId(null)}
              />
            )
            // SVG anchors take focus themselves, so the path only handles hover.
            return games ? (
              <Link
                key={slice.id}
                {...gamesLink(games, { player_id: games.player_id, color: slice.id })}
                aria-label={`${label}. Show games`}
                className="outline-none"
                onFocus={() => setActiveId(slice.id)}
                onBlur={() => setActiveId(null)}
              >
                {path}
              </Link>
            ) : (
              <g key={slice.id}>{path}</g>
            )
          })}
          {active && activeRow ? (
            <>
              <text
                x="100"
                y="94"
                textAnchor="middle"
                className="fill-base-content text-[10px] font-bold uppercase"
              >
                {activeRow.name}
              </text>
              <text
                x="100"
                y="108"
                textAnchor="middle"
                className="fill-base-content text-[12px] font-bold tabular-nums"
              >
                {active.percentage.toFixed(1)}%
              </text>
              <text
                x="100"
                y="120"
                textAnchor="middle"
                className="fill-base-content/60 text-[8px] tabular-nums"
              >
                {`${active.games} ${active.games === 1 ? "game" : "games"}`}
              </text>
            </>
          ) : (
            <>
              <text
                x="100"
                y="96"
                textAnchor="middle"
                className="fill-base-content text-[10px] font-bold uppercase"
              >
                Color
              </text>
              <text x="100" y="112" textAnchor="middle" className="fill-base-content/60 text-[9px]">
                appearances
              </text>
            </>
          )}
        </svg>
      )}
    </section>
  )
}
