import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { Archive, ExternalLink, Trophy } from "lucide-react"
import { useState, type FormEvent } from "react"
import { DeckFormFields, type DeckFormValue } from "@/features/decks/deck-form-fields"
import { DeckStats } from "@/components/stats/deck-stats"
import { Switch } from "@/components/ui/switch"
import { api, ApiError } from "@/lib/api"
import { cardSnapshot, getCard, selectCatalogCard, type CardSummary } from "@/lib/cards"
import { useCurrentUser } from "@/lib/auth"
import { formatDate, invalidateGameRelated } from "@/features/games/games"
import { canManageDeck, getDeck, isRetired, type DeckDetail } from "@/features/decks/decks"
import { DeleteDeckCard } from "@/features/decks/delete-deck"
import { RetireDeckCard } from "@/features/decks/retire-deck"
import { DeckCommanders } from "@/features/decks/deck-commanders"

export const Route = createFileRoute("/decks/$deckId")({ component: DeckDetailPage })

function DeckDetailPage() {
  const { deckId } = Route.useParams()
  const query = useQuery({ queryKey: ["decks", deckId], queryFn: () => getDeck(deckId) })
  const viewer = useCurrentUser()
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return <div className="alert alert-error">Deck not found.</div>
  const deck = query.data
  return (
    <div className="flex flex-col gap-8">
      <PageHeader
        eyebrow={
          deck.player ? (
            <Link to="/players/$playerId" params={{ playerId: String(deck.player.id) }}>
              {deck.player.name}
            </Link>
          ) : undefined
        }
        title={deck.name}
        description={<DeckCommanders deck={deck} />}
        backgroundImageUrl={deck.commander_art_crop_url}
        actions={
          isRetired(deck) || deck.decklist_url ? (
            <>
              {isRetired(deck) && (
                <span className="badge badge-neutral gap-1">
                  <Archive className="size-3" /> Retired
                </span>
              )}
              {deck.decklist_url && (
                <a
                  href={deck.decklist_url}
                  target="_blank"
                  rel="noreferrer"
                  className="btn btn-outline btn-sm"
                >
                  <ExternalLink className="size-4" /> Open deck list
                </a>
              )}
            </>
          ) : undefined
        }
      >
        <div className="mt-4 flex flex-col items-start gap-5">
          <ColorIdentity colors={deck.color_identity} className="text-lg" />
          <div className="stats border-base-300 bg-base-100/60 border">
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
      </PageHeader>
      <DeckStats deckId={deckId} />
      {canManageDeck(viewer.data, deck) ? (
        <DeckEditForm key={deck.id} deck={deck} />
      ) : (
        <p className="text-base-content/60 text-sm">
          Only the linked owner or an administrator can edit this deck.
        </p>
      )}
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
      {canManageDeck(viewer.data, deck) && (
        <div className="grid gap-4 lg:grid-cols-2">
          <RetireDeckCard deck={deck} />
          <DeleteDeckCard deck={deck} />
        </div>
      )}
    </div>
  )
}

export function DeckEditForm({ deck }: { deck: DeckDetail }) {
  const commander = useQuery({
    queryKey: ["cards", deck.commander_card_id],
    queryFn: () => getCard(deck.commander_card_id!),
    enabled: Boolean(deck.commander_card_id),
    retry: false,
  })
  const partner = useQuery({
    queryKey: ["cards", deck.partner_card_id],
    queryFn: () => getCard(deck.partner_card_id!),
    enabled: Boolean(deck.partner_card_id),
    retry: false,
  })

  if (
    (deck.commander_card_id && commander.isPending) ||
    (deck.partner_card_id && partner.isPending)
  ) {
    return <span className="loading loading-spinner" aria-label="Loading deck cards" />
  }

  return (
    <DeckEditFormReady
      deck={deck}
      commander={selectedCard(
        commander.data,
        deck.commander_card_id,
        deck.commander_name,
        deck.commander_printing_id,
      )}
      partner={selectedCard(
        partner.data,
        deck.partner_card_id,
        deck.partner_name,
        deck.partner_printing_id,
      )}
    />
  )
}

function selectedCard(
  card: CardSummary | undefined,
  id: string | null,
  name: string | null,
  printingId?: string | null,
) {
  const selected = card ? selectCatalogCard(card) : cardSnapshot(id, name)
  return selected ? { ...selected, printing_id: printingId ?? null } : null
}

function DeckEditFormReady({
  deck,
  commander,
  partner,
}: {
  deck: DeckDetail
  commander: DeckFormValue["commander"]
  partner: DeckFormValue["partner"]
}) {
  const queryClient = useQueryClient()
  const [name, setName] = useState(deck.name)
  const [nameEditVersion, setNameEditVersion] = useState(0)
  const [includedForPlay, setIncludedForPlay] = useState(deck.included_for_play ?? true)
  const [details, setDetails] = useState<DeckFormValue>({
    commander,
    partner,
    colorIdentity: deck.color_identity,
    decklistUrl: deck.decklist_url ?? "",
  })
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: DeckDetail }>(`/api/decks/${deck.id}`, {
        method: "PATCH",
        body: JSON.stringify({
          deck: {
            name: name.trim(),
            commander_card_id: details.commander?.catalog_id ?? null,
            commander_name: details.commander?.name ?? null,
            commander_printing_id: details.commander?.printing_id ?? null,
            partner_card_id: details.partner?.catalog_id ?? null,
            partner_name: details.partner?.name ?? null,
            partner_printing_id: details.partner?.printing_id ?? null,
            color_identity: details.colorIdentity,
            decklist_url: details.decklistUrl.trim() || null,
            included_for_play: includedForPlay,
          },
        }),
      }).then((body) => body.data),
    onSuccess: (saved) => {
      queryClient.setQueryData(["decks", String(deck.id)], saved)
      void invalidateGameRelated(queryClient)
    },
  })
  const error = mutation.error instanceof ApiError ? mutation.error : null

  function submit(event: FormEvent) {
    event.preventDefault()
    mutation.mutate()
  }

  return (
    <section className="card border-base-300 bg-base-200 border">
      <form className="card-body gap-4 p-4 sm:p-6" onSubmit={submit}>
        <div>
          <h2 className="text-xl font-bold">Deck details</h2>
          <p className="text-base-content/60 text-sm">
            Select catalog cards or import details from a public deck link.
          </p>
        </div>
        <div className="grid min-w-0 gap-4 sm:grid-cols-2">
          <label className="form-control sm:col-span-2">
            <span className="label-text mb-1 text-sm font-medium">Deck name</span>
            <input
              className="input input-bordered w-full"
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                setNameEditVersion((version) => version + 1)
              }}
              required
            />
          </label>
          <DeckFormFields
            value={details}
            allowPrintings
            onChange={(patch) => {
              setDetails((current) => ({ ...current, ...patch }))
              mutation.reset()
            }}
            onResolvedName={setName}
            manualEditVersion={nameEditVersion}
          />
          <label className="flex items-center justify-between gap-4 sm:col-span-2">
            <span>
              <span className="block text-sm font-bold">Include in deck chooser</span>
              <span className="text-base-content/60 block text-sm">
                Retired decks are always excluded from random picks.
              </span>
            </span>
            <Switch
              checked={includedForPlay}
              onCheckedChange={setIncludedForPlay}
              aria-label="Include in deck chooser"
            />
          </label>
        </div>
        {mutation.isError && (
          <div role="alert" className="alert alert-error">
            {error?.detail ??
              ([
                ...(error?.fieldErrors("commander_printing_id") ?? []),
                ...(error?.fieldErrors("partner_printing_id") ?? []),
              ].join(". ") ||
                "Could not save deck details.")}
          </div>
        )}
        {mutation.isSuccess && <p className="text-success text-sm">Deck details saved.</p>}
        <div className="card-actions justify-end">
          <button className="btn btn-primary" disabled={mutation.isPending}>
            {mutation.isPending ? "Saving…" : "Save deck"}
          </button>
        </div>
      </form>
    </section>
  )
}
