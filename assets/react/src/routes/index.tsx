import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { Clock3, Crown, Gamepad2, RotateCcw, Trophy } from "lucide-react"
import { useState } from "react"
import { ColorIdentity } from "@/components/mana-symbols"
import { BarChart } from "@/components/stats/charts"
import { StatCard } from "@/components/stats/stat-card"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import { formatDate } from "@/lib/games"
import {
  LEADERBOARD_MIN_GAMES,
  getOverviewStats,
  leaderboardRows,
  sinceLabel,
  sortByMetric,
  type ColorMetric,
  type RecordStat,
} from "@/lib/stats"

export const Route = createFileRoute("/")({ component: HomePage })

function HomePage() {
  const query = useQuery({ queryKey: ["stats", "overview"], queryFn: getOverviewStats })
  if (query.isPending)
    return (
      <div className="grid min-h-64 place-items-center">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  if (query.isError)
    return <div className="alert alert-error">Could not load playgroup statistics.</div>
  const stats = query.data

  if (stats.games_count === 0) return <EmptyDashboard />
  const leaderboard = leaderboardRows(stats.leaderboard)
  const leader = leaderboard[0]
  const since = sinceLabel(stats.detailed_stats_from)

  return (
    <div className="flex flex-col gap-6 sm:gap-8">
      <header>
        <p className="text-primary text-xs font-bold tracking-[0.2em] uppercase">Playgroup pulse</p>
        <h1 className="mt-1 text-3xl font-black tracking-tight sm:text-5xl">
          The table, by the numbers.
        </h1>
        <p className="text-base-content/60 mt-2">
          Every rivalry, hot streak, and improbable topdeck.
        </p>
      </header>

      <section aria-label="Highlights" className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label="Games played"
          value={stats.games_count}
          detail="all time"
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

      <div className="grid gap-4 lg:grid-cols-[1.35fr_1fr]">
        <section className="border-base-300 bg-base-200/60 overflow-hidden rounded-xl border">
          <div className="border-base-300 flex items-center justify-between border-b p-5">
            <div>
              <p className="text-primary text-xs font-bold uppercase">Standings</p>
              <h2 className="text-xl font-bold">Playgroup leaderboard</h2>
            </div>
            <Trophy className="text-accent size-6" />
          </div>
          <div className="divide-base-300 divide-y">
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
                className="hover:bg-base-300/40 grid grid-cols-[2rem_1fr_auto] items-center gap-3 px-5 py-3 transition-colors"
              >
                <span className="text-base-content/35 font-mono font-bold">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span>
                  <strong className="block">{player.name}</strong>
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
          <BarChart rows={stats.commanders.slice(0, 6)} value="games" />
        </section>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
          <p className="text-primary text-xs font-bold uppercase">Opening advantage</p>
          <h2 className="mb-5 text-xl font-bold">
            Wins by seat
            {since && (
              <span className="text-base-content/50 ml-2 text-sm font-medium">{since}</span>
            )}
          </h2>
          <BarChart rows={stats.seat_win_rates} />
        </section>
        <ColorSection rows={stats.color_win_rates} />
      </div>

      <section>
        <div className="mb-3 flex items-end justify-between">
          <div>
            <p className="text-primary text-xs font-bold uppercase">Fresh from the table</p>
            <h2 className="text-xl font-bold">Recent games</h2>
          </div>
          <Link to="/games" className="btn btn-ghost btn-sm">
            View all
          </Link>
        </div>
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {stats.recent_games.map((game) => (
            <Link
              key={game.id}
              to="/games/$gameId"
              params={{ gameId: String(game.id) }}
              className="border-base-300 bg-base-200/60 hover:border-primary/40 rounded-xl border p-4 transition-colors"
            >
              <strong>{game.winner ? `${game.winner.name} won` : "Draw game"}</strong>
              <p className="text-base-content/55 mt-1 text-sm">
                {formatDate(game.played_at)} · {game.players} players
                {game.duration_minutes ? ` · ${game.duration_minutes}m` : ""}
              </p>
            </Link>
          ))}
        </div>
      </section>
    </div>
  )
}

const colorMetricLabels: Record<ColorMetric, string> = {
  games: "Popularity",
  win_rate: "Win rate",
}

function ColorSection({ rows }: { rows: RecordStat[] }) {
  const [metric, setMetric] = useState<ColorMetric>("games")
  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-primary text-xs font-bold uppercase">Color check</p>
          <h2 className="text-xl font-bold">
            {metric === "games" ? "Most played colors" : "Color win rates"}
          </h2>
        </div>
        <ToggleGroup
          type="single"
          value={metric}
          onValueChange={(value) => value && setMetric(value as ColorMetric)}
          aria-label="Color metric"
          className="join"
        >
          {(Object.keys(colorMetricLabels) as ColorMetric[]).map((value) => (
            <ToggleGroupItem
              key={value}
              value={value}
              className={cn("btn btn-xs join-item", metric === value ? "btn-primary" : "btn-ghost")}
            >
              {colorMetricLabels[value]}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      <BarChart
        rows={sortByMetric(rows, metric).slice(0, 6)}
        value={metric}
        renderLabel={(row) => (
          <span className="inline-flex items-center gap-2">
            <ColorIdentity colors={String(row.id)} />
            <span>{row.name}</span>
          </span>
        )}
      />
    </section>
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
