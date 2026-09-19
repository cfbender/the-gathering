import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { Library } from "lucide-react"
import { getDecks } from "@/lib/games"

export const Route = createFileRoute("/decks/")({ component: DecksPage })

function DecksPage() {
  const query = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  return (
    <div className="flex flex-col gap-6">
      <div>
        <p className="text-primary text-sm font-semibold uppercase">The arsenal</p>
        <h1 className="text-3xl font-bold">Decks</h1>
      </div>
      {query.isPending && <span className="loading loading-spinner" />}
      {query.data?.length === 0 && (
        <div className="hero border-base-300 bg-base-200 rounded-box border py-16 text-center">
          <div className="hero-content flex-col">
            <Library className="text-primary size-10" />
            <h2 className="text-2xl font-bold">No decks yet</h2>
            <p className="text-base-content/70">Create a deck inline while logging a game.</p>
            <Link to="/games/new" className="btn btn-primary">
              Log a game
            </Link>
          </div>
        </div>
      )}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {query.data?.map((deck) => (
          <Link
            key={deck.id}
            to="/decks/$deckId"
            params={{ deckId: String(deck.id) }}
            className="card border-base-300 bg-base-200 hover:border-primary border"
          >
            <div className="card-body gap-2 p-5">
              <span className="text-primary text-xs font-bold uppercase">{deck.player?.name}</span>
              <h2 className="text-xl font-bold">{deck.name}</h2>
              <p className="text-base-content/70">
                {deck.commander_name}
                {deck.partner_name && ` + ${deck.partner_name}`}
              </p>
              {deck.color_identity && (
                <span className="font-mono text-xs tracking-widest">{deck.color_identity}</span>
              )}
            </div>
          </Link>
        ))}
      </div>
    </div>
  )
}
