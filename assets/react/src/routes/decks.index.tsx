import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { Dices, Library } from "lucide-react"
import { CardArtBackground } from "@/components/card-art-background"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { getDecks } from "@/features/decks/decks"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import {
  type DeckScope,
  ownDecks,
  parseDeckScope,
  resolveDeckScope,
} from "@/features/decks/own-decks"

export const Route = createFileRoute("/decks/")({
  // `?scope=mine|all` persists the toggle across reloads and shared links.
  validateSearch: (search: Record<string, unknown>) => ({ scope: parseDeckScope(search.scope) }),
  component: DecksPage,
})

function DecksPage() {
  const query = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  const viewer = useCurrentUser()
  const { scope: choice } = Route.useSearch()
  const navigate = Route.useNavigate()

  const decks = query.data ?? []
  const mine = ownDecks(decks, viewer.data?.id)
  const scope = resolveDeckScope(choice, mine.length)
  const shown = scope === "mine" ? mine : decks

  function setScope(value: DeckScope) {
    void navigate({ search: { scope: value }, replace: true })
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="The arsenal"
        title="Decks"
        actions={
          query.data && (
            <>
              <Link to="/decks/choose" className="btn btn-primary btn-sm">
                <Dices className="size-4" /> Choose a deck
              </Link>
              <ToggleGroup
                type="single"
                value={scope}
                onValueChange={(value) => value && setScope(value as DeckScope)}
                aria-label="Which decks to show"
                className="join"
              >
                {(
                  [
                    ["mine", `My decks (${mine.length})`],
                    ["all", `Everyone (${decks.length})`],
                  ] as const
                ).map(([value, label]) => (
                  <ToggleGroupItem
                    key={value}
                    value={value}
                    className={cn(
                      "btn btn-sm join-item",
                      scope === value ? "btn-primary" : "btn-ghost",
                    )}
                  >
                    {label}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </>
          )
        }
      />
      {query.isPending && <span className="loading loading-spinner" />}
      {query.data && shown.length === 0 && (
        <EmptyPanel
          icon={<Library className="size-10" />}
          title={scope === "mine" ? "No decks of yours yet" : "No decks yet"}
          description={
            scope === "mine"
              ? "Decks you log under your linked player show up here."
              : "Create a deck inline while logging a game."
          }
          action={
            <Link to="/games/new" className="btn btn-primary">
              Log a game
            </Link>
          }
        />
      )}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {shown.map((deck) => (
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
                <DeckCommanders deck={deck} />
              </p>
              <ColorIdentity colors={deck.color_identity} />
            </div>
          </Link>
        ))}
      </div>
    </div>
  )
}
