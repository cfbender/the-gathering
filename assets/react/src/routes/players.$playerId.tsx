import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { Trophy } from "lucide-react"
import { formatDate, getPlayer } from "@/lib/games"

export const Route = createFileRoute("/players/$playerId")({ component: PlayerDetailPage })

function PlayerDetailPage() {
  const { playerId } = Route.useParams()
  const query = useQuery({ queryKey: ["players", playerId], queryFn: () => getPlayer(playerId) })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return <div className="alert alert-error">Player not found.</div>
  const player = query.data
  return (
    <div className="flex flex-col gap-8">
      <div>
        <p className="text-primary text-sm font-semibold uppercase">Player profile</p>
        <h1 className="text-4xl font-bold">{player.name}</h1>
        <div className="stats border-base-300 bg-base-200 mt-4 border">
          <div className="stat">
            <div className="stat-title">Games</div>
            <div className="stat-value text-2xl">{player.games_played}</div>
          </div>
          <div className="stat">
            <div className="stat-title">Wins</div>
            <div className="stat-value text-success text-2xl">{player.wins}</div>
          </div>
        </div>
      </div>
      <section>
        <h2 className="mb-3 text-xl font-bold">Decks</h2>
        {player.decks?.length === 0 && (
          <p className="text-base-content/60">No decks recorded yet.</p>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          {player.decks?.map((deck) => (
            <Link
              key={deck.id}
              to="/decks/$deckId"
              params={{ deckId: String(deck.id) }}
              className="card border-base-300 bg-base-200 border"
            >
              <div className="card-body p-4">
                <strong>{deck.name}</strong>
                <span className="text-base-content/60 text-sm">{deck.commander_name}</span>
              </div>
            </Link>
          ))}
        </div>
      </section>
      <section>
        <h2 className="mb-3 text-xl font-bold">Recent games</h2>
        {player.recent_games?.length === 0 && (
          <p className="text-base-content/60">No games played yet.</p>
        )}
        <div className="divide-base-300 border-base-300 bg-base-200 divide-y rounded-lg border">
          {player.recent_games?.map((game) => (
            <Link
              key={game.id}
              to="/games/$gameId"
              params={{ gameId: String(game.id) }}
              className="flex items-center justify-between p-4"
            >
              <span>
                <strong>{formatDate(game.played_at)}</strong>
                <span className="text-base-content/60 block text-sm">
                  {game.deck?.name ?? "Unknown deck"}
                </span>
              </span>
              {game.result === "win" && (
                <span className="text-success flex items-center gap-1 font-bold">
                  <Trophy className="size-4" /> Win
                </span>
              )}
            </Link>
          ))}
        </div>
      </section>
    </div>
  )
}
