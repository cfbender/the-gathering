import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { Library } from "lucide-react"
import { useState } from "react"
import { CardArtBackground } from "@/components/card-art-background"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { getDecks, type Deck } from "@/lib/games"

export const Route = createFileRoute("/decks/")({ component: DecksPage })

type Scope = "mine" | "all"

/** Decks owned by the viewer's linked player. */
export function ownDecks(decks: Deck[], viewerId: number | undefined) {
  return decks.filter((deck) => viewerId !== undefined && deck.player?.user_id === viewerId)
}

function DecksPage() {
  const query = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  const viewer = useCurrentUser()
  const [choice, setChoice] = useState<Scope | null>(null)

  const decks = query.data ?? []
  const mine = ownDecks(decks, viewer.data?.id)
  // Default to the viewer's decks; fall back to everyone's when they have none
  // (no linked player yet, or nothing logged) so the page is never empty by default.
  const scope: Scope = choice ?? (mine.length > 0 ? "mine" : "all")
  const shown = scope === "mine" ? mine : decks

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="The arsenal"
        title="Decks"
        actions={
          query.data && (
            <ToggleGroup
              type="single"
              value={scope}
              onValueChange={(value) => value && setChoice(value as Scope)}
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
