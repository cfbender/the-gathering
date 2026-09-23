import { Link } from "@tanstack/react-router"
import { UsersRound } from "lucide-react"
import { formatDate } from "@/features/games/games"
import { cn } from "@/lib/cn"
import type { OverviewStats } from "@/lib/stats"

export function RecentGames({ games }: { games: OverviewStats["recent_games"] }) {
  return (
    <section aria-label="Recent games">
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
        {games.map((game) => (
          <Link
            key={game.id}
            to="/games/$gameId"
            params={{ gameId: String(game.id) }}
            className="border-base-300 bg-base-200/60 hover:border-primary/40 flex min-h-36 flex-col rounded-xl border p-4 transition-colors"
          >
            <strong>{game.winner ? `${game.winner.name} won` : "Draw game"}</strong>
            <p className="text-base-content/55 mt-1 text-sm">
              {formatDate(game.played_at)} · {game.players} players
              {game.duration_minutes ? ` · ${game.duration_minutes}m` : ""}
            </p>
            <ul aria-label="Commanders at the table" className="mt-auto flex flex-wrap gap-2 pt-4">
              {game.commanders.map((commander, index) => {
                const label = `${commander.player_name}: ${commander.name ?? "Unknown commander"}${commander.winner ? " (winner)" : ""}`
                return (
                  <li
                    key={index}
                    aria-label={label}
                    title={label}
                    className={cn(
                      "bg-base-300 border-base-content/15 relative grid size-10 shrink-0 place-items-center overflow-hidden rounded-full border",
                      commander.winner && "ring-accent ring-2 ring-offset-2 ring-offset-base-200",
                    )}
                  >
                    <UsersRound aria-hidden="true" className="text-base-content/40 size-4" />
                    {commander.art_crop_url && (
                      <img
                        src={commander.art_crop_url}
                        alt=""
                        loading="lazy"
                        decoding="async"
                        className="absolute inset-0 size-full object-cover"
                        onError={(event) => (event.currentTarget.hidden = true)}
                      />
                    )}
                  </li>
                )
              })}
            </ul>
          </Link>
        ))}
      </div>
    </section>
  )
}
