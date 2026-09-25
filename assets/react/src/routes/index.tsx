import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { Clock3, Crown, Gamepad2, RotateCcw, Trophy } from "lucide-react"
import { ActivityCalendar } from "@/components/stats/activity-calendar"
import { BarChart } from "@/components/stats/charts"
import { ColorSection } from "@/components/stats/color-section"
import { ColorWheel } from "@/components/stats/color-wheel"
import { EloSection } from "@/components/stats/elo-section"
import { GameLengths } from "@/components/stats/game-lengths"
import { KillStats } from "@/components/stats/kill-stats"
import { MatchupHeatmap } from "@/components/stats/matchup-heatmap"
import { RecentGames } from "@/components/stats/recent-games"
import { StatCard } from "@/components/stats/stat-card"
import { StatsRangeToggle } from "@/components/stats/stats-range-toggle"
import { WinConditions } from "@/components/stats/win-conditions"
import {
  LEADERBOARD_MIN_GAMES,
  getOverviewStats,
  leaderboardRows,
  sinceLabel,
  statsQueryKey,
} from "@/lib/stats"
import { statsRangeDetails, useStatsRange } from "@/lib/stats-range"
import { detailedScope, gamesLink } from "@/features/games/game-filters"

export const Route = createFileRoute("/")({ component: HomePage })

function HomePage() {
  const { range, params } = useStatsRange()
  const query = useQuery({
    queryKey: statsQueryKey(params, "overview"),
    queryFn: () => getOverviewStats(params),
  })
  if (query.isPending)
    return (
      <div className="grid min-h-64 place-items-center">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  if (query.isError)
    return <div className="alert alert-error">Could not load playgroup statistics.</div>
  const stats = query.data

  if (stats.games_count === 0 && range === "all") return <EmptyDashboard />
  const leaderboard = leaderboardRows(stats.leaderboard)
  const leader = leaderboard[0]
  const since = sinceLabel(stats.detailed_stats_from)
  const games = { date_from: params.date_from }
  const detailedGames = detailedScope(games, stats.detailed_stats_from)

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-primary text-xs font-bold tracking-[0.2em] uppercase">
            Playgroup pulse
          </p>
          <h1 className="mt-1 text-3xl font-black tracking-tight sm:text-5xl">
            The table, by the numbers.
          </h1>
          <p className="text-base-content/60 mt-2">
            Every rivalry, hot streak, and improbable topdeck.
          </p>
        </div>
        <StatsRangeToggle className="shrink-0" />
      </header>

      {stats.games_count === 0 && (
        <div className="alert">
          <Gamepad2 className="size-5" />
          <span>No games in the {statsRangeDetails[range]}. Try a wider range.</span>
        </div>
      )}

      <section aria-label="Highlights" className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label="Games played"
          value={stats.games_count}
          detail={statsRangeDetails[range]}
          icon={<Gamepad2 className="size-4" />}
        />
        <StatCard
          label="Top win rate"
          value={`${leader?.win_rate ?? 0}%`}
          detail={leader?.name}
          icon={<Crown className="text-accent size-4" />}
        />
        <StatCard
          label="Avg. length"
          value={stats.average_duration_minutes ? `${stats.average_duration_minutes}m` : "—"}
          detail={since ?? "from first draw"}
          icon={<Clock3 className="size-4" />}
        />
        <StatCard
          label="Avg. turns"
          value={stats.average_turns ?? "—"}
          detail={since ?? "per game"}
          icon={<RotateCcw className="size-4" />}
        />
      </section>

      <div className="space-y-4">
        <section
          aria-label="Playgroup leaderboard"
          className="border-base-300 bg-base-200/60 overflow-hidden rounded-xl border"
        >
          <div className="border-base-300 flex items-center justify-between gap-3 border-b p-5">
            <div>
              <p className="text-primary text-xs font-bold uppercase">Standings</p>
              <h2 className="text-xl font-bold">Playgroup leaderboard</h2>
            </div>
            <div className="flex items-center gap-2">
              <Trophy className="text-accent size-6" />
              <Link to="/players" className="btn btn-ghost btn-sm">
                View all
              </Link>
            </div>
          </div>
          <div
            className="grid md:grid-flow-col md:grid-cols-2"
            style={{
              gridTemplateRows: `repeat(${Math.max(1, Math.ceil(leaderboard.length / 2))}, auto)`,
            }}
          >
            {leaderboard.length === 0 && (
              <p className="text-base-content/55 p-5 text-sm">
                Players appear here after {LEADERBOARD_MIN_GAMES} games.
              </p>
            )}
            {leaderboard.map((player, index) => (
              <Link
                key={player.id}
                to="/players/$playerId"
                params={{ playerId: String(player.id) }}
                className="border-base-300 hover:bg-base-300/40 grid grid-cols-[2rem_minmax(0,1fr)_auto] items-center gap-3 border-t px-5 py-3 transition-colors"
              >
                <span className="text-base-content/35 font-mono font-bold">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span>
                  <strong className="block truncate" title={player.name}>
                    {player.name}
                  </strong>
                  <small className="text-base-content/55">
                    {player.wins}–{player.losses}–{player.draws} · {player.games} games
                  </small>
                </span>
                <strong className="text-primary text-lg tabular-nums">{player.win_rate}%</strong>
              </Link>
            ))}
          </div>
        </section>

        <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
          <div className="mb-5 flex items-start justify-between gap-3">
            <div>
              <p className="text-primary text-xs font-bold uppercase">The meta</p>
              <h2 className="text-xl font-bold">Most played commanders</h2>
            </div>
            <Link to="/commanders" className="btn btn-ghost btn-sm">
              View all
            </Link>
          </div>
          <BarChart
            rows={stats.commanders}
            value="games"
            columns={2}
            linkTo={(row) => gamesLink(games, { commander: row.name })}
          />
        </section>
        <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
          <p className="text-primary text-xs font-bold uppercase">Opening advantage</p>
          <h2 className="mb-5 text-xl font-bold">
            Wins by seat
            {since && (
              <span className="text-base-content/50 ml-2 text-sm font-medium">{since}</span>
            )}
          </h2>
          <BarChart
            rows={stats.seat_win_rates}
            columns={2}
            linkTo={(row) => gamesLink(detailedGames, { winner_seat: row.id })}
          />
        </section>
      </div>

      <EloSection players={stats.elo} />

      <div className="grid gap-4 md:grid-cols-2">
        <KillStats stats={stats.kills} />
        <WinConditions stats={stats.win_conditions} games={games} />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <ColorSection
          rows={stats.color_win_rates}
          games={games}
          action={
            <Link to="/colors" className="btn btn-ghost btn-sm">
              View all
            </Link>
          }
        />
        <ColorWheel rows={stats.color_exposure} eyebrow="Playgroup colors" games={games} />
      </div>

      <MatchupHeatmap players={stats.leaderboard} matchups={stats.matchups} games={games} />

      <GameLengths
        gameLengths={stats.game_lengths}
        averageDuration={stats.average_duration_minutes}
        averageTurns={stats.average_turns}
        since={since}
        games={detailedGames}
      />

      <ActivityCalendar gameTimes={stats.game_times} games={games} />

      <RecentGames games={stats.recent_games} />
    </div>
  )
}

function EmptyDashboard() {
  return (
    <section className="border-base-300 bg-base-200/60 mx-auto max-w-xl rounded-2xl border p-8 text-center sm:p-12">
      <Gamepad2 className="text-primary mx-auto size-10" />
      <h1 className="mt-4 text-3xl font-black">Your stats start at game one.</h1>
      <p className="text-base-content/60 mt-2">
        Record a game and this dashboard will turn it into standings, trends, and matchup history.
      </p>
      <Link to="/games/new" className="btn btn-primary mt-6">
        Record a game
      </Link>
    </section>
  )
}
