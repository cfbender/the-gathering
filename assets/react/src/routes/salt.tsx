import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { SatisfactionDistribution } from "@/components/stats/satisfaction-stats"
import { StatCard } from "@/components/stats/stat-card"
import { StatsRangeToggle } from "@/components/stats/stats-range-toggle"
import {
  SatisfactionBadge,
  SatisfactionFace,
  satisfactionLevel,
} from "@/features/satisfaction/satisfaction"
import { getSaltStats, LEADERBOARD_MIN_GAMES, statsQueryKey, type SaltStats } from "@/lib/stats"
import { statsRangeDetails, useStatsRange } from "@/lib/stats-range"

export const Route = createFileRoute("/salt")({ component: SaltPage })

type CommanderRow = SaltStats["commanders"][number]

/** Ratings a player or commander needs before it is ranked. */
const MIN_RATINGS = LEADERBOARD_MIN_GAMES

function formatShortDate(value: string) {
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(
    new Date(value),
  )
}

function monthLabel(month: string) {
  return new Intl.DateTimeFormat(undefined, { month: "short", year: "2-digit" }).format(
    new Date(`${month}-01T00:00:00`),
  )
}

function CommanderList({
  title,
  description,
  rows,
}: {
  title: string
  description: string
  rows: CommanderRow[]
}) {
  return (
    <section className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5">
      <h2 className="font-bold">{title}</h2>
      <p className="text-base-content/60 mb-4 text-xs">{description}</p>
      {rows.length === 0 ? (
        <p className="text-base-content/60 text-sm">
          Commanders need {MIN_RATINGS} opponent ratings to rank.
        </p>
      ) : (
        <ol className="grid gap-2">
          {rows.map((commander, index) => (
            <li key={commander.id}>
              <Link
                to="/commanders/$commanderId"
                params={{ commanderId: commander.id }}
                className="border-base-300 hover:border-primary relative flex items-center gap-3 overflow-hidden rounded-lg border px-3 py-2 transition-colors"
              >
                <CardArtBackground imageUrl={commander.art_crop_url} />
                <span
                  aria-hidden="true"
                  className="from-base-100/95 via-base-100/80 to-base-100/20 absolute inset-0 bg-gradient-to-r"
                />
                <span className="text-primary relative z-10 w-5 text-xs font-bold">
                  #{index + 1}
                </span>
                <span className="relative z-10 min-w-0 flex-1">
                  <span className="flex items-center gap-2">
                    <strong className="truncate" title={commander.name}>
                      {commander.name}
                    </strong>
                    <ColorIdentity colors={commander.color_identity ?? ""} className="text-xs" />
                  </span>
                  <span className="text-base-content/70 block text-xs">
                    {commander.opponent_ratings} opponent ratings · {commander.games}{" "}
                    {commander.games === 1 ? "game" : "games"}
                    {commander.average !== null &&
                      ` · pilots felt ${satisfactionLevel(commander.average).label}`}
                  </span>
                </span>
                <SatisfactionBadge
                  average
                  value={commander.opponent_average}
                  ratings={commander.opponent_ratings}
                  className="bg-base-100/80 relative z-10 rounded-full px-2 py-1"
                />
              </Link>
            </li>
          ))}
        </ol>
      )}
    </section>
  )
}

function SaltPage() {
  const { range, params } = useStatsRange()
  const query = useQuery({
    queryKey: statsQueryKey(params, "salt"),
    queryFn: () => getSaltStats(params),
  })
  const stats = query.data
  const rankedCommanders = (stats?.commanders ?? []).filter(
    (row) => row.opponent_average !== null && row.opponent_ratings >= MIN_RATINGS,
  )
  const rankedPlayers = (stats?.players ?? []).filter(
    (row) => row.average !== null && row.ratings >= MIN_RATINGS,
  )
  const commanderCount = Math.min(5, Math.ceil(rankedCommanders.length / 2))

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Salt"
        title="Salt mine"
        description="How much everyone enjoyed their games, rated from awful to rad with one click when a player is knocked out or the game ends."
        actions={<StatsRangeToggle />}
      />
      {query.isPending && <span className="loading loading-spinner" />}
      {query.isError && <div className="alert alert-error">Could not load salt stats.</div>}
      {stats && stats.summary.ratings === 0 && (
        <EmptyPanel
          icon={<SatisfactionFace value={3} className="size-10" />}
          title={
            range === "all" ? "No ratings yet" : `No ratings in the ${statsRangeDetails[range]}`
          }
          description="Ratings arrive from the webcam table, the tabletop, and each game's page, where every player can rate their own seat."
        />
      )}
      {stats && stats.summary.ratings > 0 && (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            <StatCard
              label="Average mood"
              value={
                <SatisfactionBadge
                  average
                  value={stats.summary.average}
                  ratings={stats.summary.ratings}
                  className="[&_svg]:size-8 [&>span]:text-3xl"
                />
              }
              detail={statsRangeDetails[range]}
            />
            <StatCard
              label="Ratings"
              value={stats.summary.ratings}
              detail={`from ${stats.rated_games} of ${stats.games_count} games`}
            />
            <StatCard
              label="Saltiest game"
              className="col-span-2 sm:col-span-1"
              value={
                stats.saltiest_game ? (
                  <Link
                    to="/games/$gameId"
                    params={{ gameId: String(stats.saltiest_game.id) }}
                    className="link link-hover flex items-center gap-2"
                  >
                    {formatShortDate(stats.saltiest_game.played_at)}
                    <SatisfactionBadge
                      average
                      value={stats.saltiest_game.average}
                      ratings={stats.saltiest_game.ratings}
                    />
                  </Link>
                ) : (
                  "—"
                )
              }
              detail={
                stats.saltiest_game
                  ? `${stats.saltiest_game.ratings} ratings · ${statsRangeDetails[range]}`
                  : "Needs a game at least two players rated"
              }
            />
          </div>
          <div className="grid gap-4 lg:grid-cols-2">
            <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
              <h2 className="mb-4 font-bold">How games felt</h2>
              <SatisfactionDistribution summary={stats.summary} />
            </section>
            <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
              <h2 className="mb-4 font-bold">Mood by month</h2>
              <ol className="grid gap-2">
                {stats.by_month.slice(-8).map((month) => (
                  <li
                    key={month.month}
                    className="grid grid-cols-[4rem_minmax(0,1fr)_auto] items-center gap-3 text-sm"
                  >
                    <span className="text-base-content/70">{monthLabel(month.month)}</span>
                    <span className="bg-base-300 h-3 overflow-hidden rounded-full">
                      <span
                        className={`block h-full rounded-full bg-current ${satisfactionLevel(month.average ?? 3).color}`}
                        style={{ width: `${((month.average ?? 0) / 5) * 100}%` }}
                      />
                    </span>
                    <SatisfactionBadge average value={month.average} ratings={month.ratings} />
                  </li>
                ))}
              </ol>
            </section>
          </div>
          <div className="grid gap-4 lg:grid-cols-2">
            <CommanderList
              title="Saltiest commanders"
              description="How the rest of the table rated games against each commander, lowest first."
              rows={rankedCommanders.slice(0, commanderCount)}
            />
            <CommanderList
              title="Least salty commanders"
              description="The commanders opponents enjoyed playing against most."
              rows={[...rankedCommanders].reverse().slice(0, commanderCount)}
            />
          </div>
          <section className="border-base-300 bg-base-200/60 overflow-x-auto rounded-xl border p-5">
            <h2 className="font-bold">Players</h2>
            <p className="text-base-content/60 mb-3 text-xs">
              Saltiest first, by how each player rated their own games. Players need {MIN_RATINGS}{" "}
              ratings to rank.
            </p>
            {rankedPlayers.length === 0 ? (
              <p className="text-base-content/60 text-sm">Nobody has enough ratings yet.</p>
            ) : (
              <table className="table">
                <thead>
                  <tr>
                    <th scope="col">Player</th>
                    <th scope="col">Their mood</th>
                    <th scope="col">Opponents' mood</th>
                    <th scope="col" className="text-right">
                      Ratings
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {rankedPlayers.map((player) => (
                    <tr key={player.id}>
                      <th scope="row">
                        <Link
                          to="/players/$playerId"
                          params={{ playerId: String(player.id) }}
                          className="link link-hover"
                        >
                          {player.name}
                        </Link>
                      </th>
                      <td>
                        <SatisfactionBadge
                          average
                          value={player.average}
                          ratings={player.ratings}
                        />
                      </td>
                      <td>
                        {player.opponent_average === null ? (
                          "—"
                        ) : (
                          <SatisfactionBadge
                            average
                            value={player.opponent_average}
                            ratings={player.opponent_ratings}
                          />
                        )}
                      </td>
                      <td className="text-right tabular-nums">
                        {player.ratings} of {player.games}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
        </>
      )}
    </div>
  )
}
