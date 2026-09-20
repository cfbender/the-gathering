import { Link } from "@tanstack/react-router"
import { EloChart } from "./elo-chart"
import { selectEloSeries } from "@/lib/elo"
import { LEADERBOARD_MIN_GAMES, type OverviewStats } from "@/lib/stats"

export function EloSection({ players }: { players: OverviewStats["elo"] }) {
  const chartSeries = selectEloSeries(players, LEADERBOARD_MIN_GAMES)

  return (
    <section aria-labelledby="ratings-heading">
      <div className="mb-3">
        <p className="text-primary text-xs font-bold uppercase">Ratings</p>
        <h2 id="ratings-heading" className="text-xl font-bold">
          Elo standings
        </h2>
      </div>
      <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.65fr)_minmax(18rem,1fr)]">
        <div className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5">
          <h3 className="mb-4 font-bold">Top players over time</h3>
          {chartSeries.length > 0 ? (
            <EloChart series={chartSeries} />
          ) : (
            <p className="text-base-content/55 text-sm">
              Players appear after {LEADERBOARD_MIN_GAMES} games.
            </p>
          )}
        </div>
        <div className="border-base-300 bg-base-200/60 overflow-hidden rounded-xl border">
          <h3 className="border-base-300 border-b px-5 py-4 font-bold">Current ratings</h3>
          <div className="divide-base-300 max-h-[28rem] divide-y overflow-y-auto">
            {players.map((player, index) => (
              <Link
                key={player.id}
                to="/players/$playerId"
                params={{ playerId: String(player.id) }}
                className="hover:bg-base-300/40 grid grid-cols-[1.5rem_minmax(0,1fr)_auto] items-center gap-3 px-5 py-3 transition-colors"
              >
                <span className="text-base-content/35 text-xs font-bold tabular-nums">
                  {index + 1}
                </span>
                <span className="min-w-0">
                  <strong className="block truncate">{player.name}</strong>
                  <small className="text-base-content/50 block truncate">
                    peak {player.peak} · {player.games} games
                  </small>
                </span>
                <strong className="text-primary tabular-nums">{player.rating}</strong>
              </Link>
            ))}
          </div>
        </div>
      </div>
      <p className="text-base-content/45 mt-3 text-xs leading-relaxed">
        Everyone starts at 1000 with K = 32. Winners score against each loser, draws split their
        comparison, and losing seats are not compared; changes are averaged across opponents so each
        game is zero-sum.
      </p>
    </section>
  )
}
