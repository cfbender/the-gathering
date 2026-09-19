import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { ExternalLink, Trophy } from "lucide-react"
import { useState, type FormEvent } from "react"
import { DeckFormFields, type DeckFormValue } from "@/components/deck-form-fields"
import { api, ApiError } from "@/lib/api"
import { cardSnapshot } from "@/lib/cards"
import { formatDate, getDeck, type Deck } from "@/lib/games"

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
      <DeckEditForm key={deck.id} deck={deck} />
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

function DeckEditForm({ deck }: { deck: Deck }) {
  const queryClient = useQueryClient()
  const [name, setName] = useState(deck.name)
  const [details, setDetails] = useState<DeckFormValue>({
    commander: cardSnapshot(
      deck.commander_card_id,
      deck.commander_name,
      deck.color_identity.split(""),
    ),
    partner: cardSnapshot(deck.partner_card_id, deck.partner_name),
    colorIdentity: deck.color_identity,
    decklistUrl: deck.decklist_url ?? "",
  })
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: Deck }>(`/api/decks/${deck.id}`, {
        method: "PATCH",
        body: JSON.stringify({
          deck: {
            name: name.trim(),
            commander_card_id: details.commander?.catalog_id,
            commander_name: details.commander?.name,
            partner_card_id: details.partner?.catalog_id,
            partner_name: details.partner?.name,
            color_identity: details.colorIdentity,
            decklist_url: details.decklistUrl.trim() || null,
          },
        }),
      }).then((body) => body.data),
    onSuccess: (saved) => {
      queryClient.setQueryData(["decks", String(deck.id)], saved)
      void queryClient.invalidateQueries({ queryKey: ["decks"] })
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
              onChange={(event) => setName(event.target.value)}
              required
            />
          </label>
          <DeckFormFields
            value={details}
            onChange={(patch) => setDetails((current) => ({ ...current, ...patch }))}
            onResolvedName={setName}
          />
        </div>
        {mutation.isError && (
          <div role="alert" className="alert alert-error">
            {error?.detail ?? "Could not save deck details."}
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
