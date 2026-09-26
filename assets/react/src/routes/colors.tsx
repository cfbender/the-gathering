import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { Palette } from "lucide-react"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { BarChart } from "@/components/stats/charts"
import { colorIdentityLink } from "@/components/stats/color-section"
import { ColorWheel } from "@/components/stats/color-wheel"
import { StatsRangeToggle } from "@/components/stats/stats-range-toggle"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { gamesLink } from "@/features/games/game-filters"
import { cn } from "@/lib/cn"
import {
  LEADERBOARD_MIN_GAMES,
  getOverviewStats,
  isColorMetric,
  sortByMetric,
  statsQueryKey,
  type ColorMetric,
  type NamedRecordRow,
} from "@/lib/stats"
import { statsRangeDetails, useStatsRange } from "@/lib/stats-range"
import { useStoredChoice } from "@/lib/stored-choice"

export const Route = createFileRoute("/colors")({ component: ColorsPage })

const metricLabels: Record<ColorMetric, string> = {
  games: "Popularity",
  win_rate: "Win rate",
}

/** Identity rows bucketed by how many colors they run, in the order the page shows them. */
const tiers = [
  { size: 1, title: "Mono-color", description: "One color, no compromises." },
  { size: 2, title: "Two colors", description: "The ten guilds." },
  { size: 3, title: "Three colors", description: "Shards and wedges." },
  { size: 4, title: "Four colors", description: "Everything but one." },
  { size: 5, title: "Five colors", description: "The whole rainbow." },
  { size: 0, title: "Colorless", description: "Decks with no color identity." },
]

function ColorsPage() {
  const { range, params } = useStatsRange()
  const query = useQuery({
    queryKey: statsQueryKey(params, "overview"),
    queryFn: () => getOverviewStats(params),
  })
  const [metric, setMetric] = useStoredChoice<ColorMetric>(
    "the-gathering:stats-metric:colors",
    "games",
    isColorMetric,
  )
  const identities = query.data?.color_win_rates ?? []
  const ranked = sortByMetric(identities, metric)
  const games = { date_from: params.date_from }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Color check"
        title="Colors"
        description="Every color identity that has hit the table, from mono-color to five-color."
        actions={
          <>
            <StatsRangeToggle />
            {query.data && (
              <ToggleGroup
                type="single"
                value={metric}
                onValueChange={(value) => value && setMetric(value as ColorMetric)}
                aria-label="Sort colors by"
                className="join"
              >
                {(Object.keys(metricLabels) as ColorMetric[]).map((value) => (
                  <ToggleGroupItem
                    key={value}
                    value={value}
                    className={cn(
                      "btn btn-sm join-item",
                      metric === value ? "btn-primary" : "btn-ghost",
                    )}
                  >
                    {metricLabels[value]}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            )}
          </>
        }
      />
      {query.isPending && <span className="loading loading-spinner" />}
      {query.isError && <div className="alert alert-error">Could not load color stats.</div>}
      {query.data && identities.length === 0 && (
        <EmptyPanel
          icon={<Palette className="size-10" />}
          title={range === "all" ? "No colors yet" : `No colors in the ${statsRangeDetails[range]}`}
          description={
            range === "all"
              ? "Color stats appear here once a game with a deck is logged."
              : "Try a wider time range to see older games."
          }
          action={
            <Link to="/games/new" className="btn btn-primary">
              Log a game
            </Link>
          }
        />
      )}
      {query.data && identities.length > 0 && (
        <>
          <div className="grid gap-4 md:grid-cols-2">
            <ColorWheel rows={query.data.color_exposure} eyebrow="Playgroup colors" games={games} />
            <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
              <p className="text-primary text-xs font-bold uppercase">By color</p>
              <h2 className="text-xl font-bold">
                {metric === "games" ? "Most played colors" : "Win rate by color"}
              </h2>
              <p className="text-base-content/60 mt-1 mb-5 text-sm">
                Multicolor decks count toward every color they include. Click a color to see its{" "}
                {metric === "games" ? "games" : "wins"}.
              </p>
              <BarChart
                rows={sortByMetric(query.data.color_exposure, metric, 0)}
                value={metric}
                linkTo={(row) =>
                  gamesLink(
                    games,
                    metric === "games" ? { color: row.id } : { winner_color: row.id },
                  )
                }
                renderLabel={(row) => (
                  <IdentityLabel
                    row={row}
                    detail={
                      metric === "games"
                        ? `${row.win_rate}% win rate`
                        : `${row.games} ${row.games === 1 ? "game" : "games"}`
                    }
                  />
                )}
              />
            </section>
          </div>

          {ranked.length === 0 && (
            <p className="text-base-content/50 text-sm">
              Play at least {LEADERBOARD_MIN_GAMES} games in a color identity to rank its win rate.
            </p>
          )}
          {tiers.map((tier) => {
            const rows = ranked.filter((row) => colorCount(row) === tier.size)
            if (rows.length === 0) return null
            return (
              <section
                key={tier.size}
                className="border-base-300 bg-base-200/60 rounded-xl border p-5"
              >
                <div className="mb-5 flex flex-wrap items-baseline justify-between gap-2">
                  <h2 className="text-xl font-bold">{tier.title}</h2>
                  <p className="text-base-content/50 text-sm">{tier.description}</p>
                </div>
                <BarChart
                  rows={rows}
                  value={metric}
                  columns={2}
                  linkTo={(row) => colorIdentityLink(games, metric, row)}
                  renderLabel={(row) => (
                    <IdentityLabel
                      row={row}
                      detail={
                        metric === "games"
                          ? `${row.win_rate}% win rate · ${row.games} ${row.games === 1 ? "game" : "games"}`
                          : `${row.wins}–${row.losses}${row.draws ? `–${row.draws}` : ""} · ${row.games} ${row.games === 1 ? "game" : "games"}`
                      }
                    />
                  )}
                />
              </section>
            )
          })}
        </>
      )}
    </div>
  )
}

function IdentityLabel({ row, detail }: { row: NamedRecordRow; detail: string }) {
  return (
    <span className="inline-flex min-w-0 items-center gap-2">
      <ColorIdentity colors={row.id === "" ? "C" : String(row.id)} />
      <span className="truncate">{row.name}</span>
      <span className="text-base-content/50 truncate text-xs font-normal">{detail}</span>
    </span>
  )
}

/** Canonical identity IDs are the WUBRG letters, so their length is the color count. */
function colorCount(row: NamedRecordRow) {
  return String(row.id).length
}
