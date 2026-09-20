import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { Library } from "lucide-react"
import { CardArtBackground } from "@/components/card-art-background"
import { getDecks } from "@/lib/games"

export const Route = createFileRoute("/decks/")({ component: DecksPage })

function DecksPage() {
  const query = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  return (
    <div className="flex flex-col gap-6">
      <PageHeader eyebrow="The arsenal" title="Decks" />
      {query.isPending && <span className="loading loading-spinner" />}
      {query.data?.length === 0 && (
        <EmptyPanel
          icon={<Library className="size-10" />}
          title="No decks yet"
          description="Create a deck inline while logging a game."
          action={
            <Link to="/games/new" className="btn btn-primary">
              Log a game
            </Link>
          }
        />
      )}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {query.data?.map((deck) => (
          <Link
            key={deck.id}
            to="/decks/$deckId"
            params={{ deckId: String(deck.id) }}
            className="card group border-base-300 bg-base-200 hover:border-primary relative overflow-hidden border transition-all hover:-translate-y-0.5 hover:shadow-xl"
          >
            <CardArtBackground imageUrl={deck.commander_art_crop_url} interactive />
            <div className="card-body text-base-content relative z-10 gap-2 p-5">
              <span className="text-primary text-xs font-bold uppercase">{deck.player?.name}</span>
              <h2 className="text-xl font-bold">{deck.name}</h2>
              <p className="text-base-content/85">
                {deck.commander_name}
                {deck.partner_name && ` + ${deck.partner_name}`}
              </p>
              <ColorIdentity colors={deck.color_identity} />
            </div>
          </Link>
        ))}
      </div>
    </div>
  )
}
