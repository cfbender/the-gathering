import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { ExternalLink, Trophy } from "lucide-react"
import { formatDate, getDeck } from "@/lib/games"

export const Route = createFileRoute("/decks/$deckId")({ component: DeckDetailPage })

function DeckDetailPage() {
  const { deckId } = Route.useParams()
  const query = useQuery({ queryKey: ["decks", deckId], queryFn: () => getDeck(deckId) })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return <div className="alert alert-error">Deck not found.</div>
  const deck = query.data
  return (
    <div className="flex flex-col gap-8">
      <div>
        <p className="text-primary text-sm font-semibold uppercase">
          {deck.player && (
            <Link to="/players/$playerId" params={{ playerId: String(deck.player.id) }}>
              {deck.player.name}
            </Link>
          )}
        </p>
        <h1 className="text-4xl font-bold">{deck.name}</h1>
        <p className="text-base-content/70 mt-2 text-xl">
          {deck.commander_name}
          {deck.partner_name && ` + ${deck.partner_name}`}
        </p>
        {deck.decklist_url && (
          <a
            href={deck.decklist_url}
            target="_blank"
            rel="noreferrer"
            className="btn btn-outline btn-sm mt-4"
          >
            <ExternalLink className="size-4" /> Open deck list
          </a>
        )}
        <div className="stats border-base-300 bg-base-200 mt-5 border">
          <div className="stat">
            <div className="stat-title">Games</div>
            <div className="stat-value text-2xl">{deck.games_played}</div>
          </div>
          <div className="stat">
            <div className="stat-title">Wins</div>
            <div className="stat-value text-success text-2xl">{deck.wins}</div>
          </div>
        </div>
      </div>
      <section>
        <h2 className="mb-3 text-xl font-bold">Recent games</h2>
        {deck.recent_games?.length === 0 && (
          <p className="text-base-content/60">This deck has not hit the table yet.</p>
        )}
        <div className="divide-base-300 border-base-300 bg-base-200 divide-y rounded-lg border">
          {deck.recent_games?.map((game) => (
            <Link
              key={game.id}
              to="/games/$gameId"
              params={{ gameId: String(game.id) }}
              className="flex items-center justify-between p-4"
            >
              <strong>{formatDate(game.played_at)}</strong>
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
