import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { Plus } from "lucide-react"
import type { FormEvent } from "react"
import { DeckFormFields } from "@/features/decks/deck-form-fields"
import {
  blankSeat,
  moveSeat,
  resultsForSeats,
  useGameDraft,
  type DraftSeat,
} from "@/features/games/use-game-draft"
import { MvpCardField } from "@/components/mvp-card-field"
import { api, ApiError } from "@/lib/api"
import { cardSnapshot } from "@/lib/cards"
import {
  getPlayers,
  invalidateGameRelated,
  type Game,
  type PlayerSummary,
} from "@/features/games/games"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import { SeatEditor } from "@/features/games/seat-editor"

interface GameFormProps {
  game?: Game
}

async function ensurePlayer(draft: DraftSeat, players: PlayerSummary[]) {
  if (draft.playerId !== null) return draft.playerId
  const existing = players.find(
    (player) => player.name.toLowerCase() === draft.playerName.trim().toLowerCase(),
  )
  if (existing) return existing.id
  const created = await api<{ data: PlayerSummary }>("/api/players", {
    method: "POST",
    body: JSON.stringify({ player: { name: draft.playerName.trim() } }),
  }).then((body) => body.data)
  players.push(created)
  return created.id
}

async function ensureDeck(draft: DraftSeat, playerId: number, decks: DeckSummary[]) {
  if (!draft.deckName.trim()) return null
  if (draft.deckId !== null) return draft.deckId
  const existing = decks.find(
    (deck) =>
      deck.player_id === playerId &&
      deck.name.toLowerCase() === draft.deckName.trim().toLowerCase(),
  )
  if (existing) return existing.id
  const created = await api<{ data: DeckSummary }>("/api/decks", {
    method: "POST",
    body: JSON.stringify({
      deck: {
        player_id: playerId,
        name: draft.deckName.trim(),
        commander_card_id: draft.commander?.catalog_id,
        commander_name: draft.commander?.name,
        partner_card_id: draft.partner?.catalog_id,
        partner_name: draft.partner?.name,
        color_identity: draft.colorIdentity,
        decklist_url: draft.decklistUrl.trim() || null,
      },
    }),
  }).then((body) => body.data)
  decks.push(created)
  return created.id
}

export function GameForm({ game }: GameFormProps) {
  return <GameFormDraft key={game?.id ?? "new-game"} game={game} />
}

function GameFormDraft({ game }: GameFormProps) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const playersQuery = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const decksQuery = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  const {
    playedAt,
    setPlayedAt,
    seats,
    setSeats,
    winnerSeatId,
    setWinnerSeatId,
    turns,
    setTurns,
    duration,
    setDuration,
    notes,
    setNotes,
    updateSeat,
  } = useGameDraft(game)

  const mutation = useMutation({
    mutationFn: async () => {
      const knownPlayers = [...(playersQuery.data ?? [])]
      const knownDecks = [...(decksQuery.data ?? [])]
      const results = resultsForSeats(seats, winnerSeatId)
      const payloadSeats = []

      for (const [index, draft] of seats.entries()) {
        const playerId = await ensurePlayer(draft, knownPlayers)
        const deckId = await ensureDeck(draft, playerId, knownDecks)
        payloadSeats.push({
          id: draft.id,
          player_id: playerId,
          deck_id: deckId,
          seat: index + 1,
          result: results[index],
          kills: draft.kills === "" ? null : Number(draft.kills),
          mvp_card_id: draft.mvpCard?.catalog_id ?? null,
          mvp_card_name: draft.mvpCard?.name ?? null,
        })
      }

      const payload = {
        game: {
          played_at: new Date(playedAt).toISOString(),
          turns: turns ? Number(turns) : null,
          duration_minutes: duration ? Number(duration) : null,
          notes: notes.trim() || null,
          seats: payloadSeats,
        },
      }
      return api<{ data: Game }>(game ? `/api/games/${game.id}` : "/api/games", {
        method: game ? "PATCH" : "POST",
        body: JSON.stringify(payload),
      }).then((body) => body.data)
    },
    onSuccess: async (saved) => {
      await invalidateGameRelated(queryClient)
      void navigate({ to: "/games/$gameId", params: { gameId: String(saved.id) } })
    },
  })

  function submit(event: FormEvent) {
    event.preventDefault()
    mutation.mutate()
  }

  const error = mutation.error instanceof ApiError ? mutation.error : null

  return (
    <form onSubmit={submit} className="flex flex-col gap-6">
      <section className="card border-base-300 bg-base-200 border">
        <div className="card-body gap-4 p-4 sm:p-6">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h2 className="text-lg font-bold">Table</h2>
              <p className="text-base-content/60 text-sm">Seat 1 takes the first turn.</p>
            </div>
            <button
              type="button"
              className="btn btn-sm btn-outline"
              disabled={seats.length >= 6}
              onClick={() => setSeats((current) => [...current, blankSeat()])}
            >
              <Plus className="size-4" /> Add seat
            </button>
          </div>

          <datalist id="player-names">
            {playersQuery.data?.map((player) => (
              <option key={player.id} value={player.name} />
            ))}
          </datalist>

          <div className="flex flex-col gap-3">
            {seats.map((seat, index) => {
              const player = playersQuery.data?.find((candidate) => candidate.id === seat.playerId)
              const playerDecks =
                decksQuery.data?.filter((deck) => deck.player_id === seat.playerId) ?? []
              const selectedDeck = playerDecks.find((deck) => deck.id === seat.deckId)
              return (
                <SeatEditor
                  key={seat.draftId}
                  seat={seat}
                  index={index}
                  seatCount={seats.length}
                  winner={winnerSeatId === seat.draftId}
                  onChooseWinner={() => setWinnerSeatId(seat.draftId)}
                  onMove={(direction) => setSeats((value) => moveSeat(value, index, direction))}
                  onRemove={() => {
                    const remaining = seats.filter((_, seatIndex) => seatIndex !== index)
                    setSeats(remaining)
                    if (winnerSeatId === seat.draftId) {
                      setWinnerSeatId(remaining[0]?.draftId ?? null)
                    }
                  }}
                >
                  <div className="grid gap-3 sm:grid-cols-2">
                    <label className="form-control">
                      <span className="label-text mb-1 text-xs font-medium">Player</span>
                      <input
                        className="input input-bordered input-sm w-full"
                        list="player-names"
                        placeholder="Choose or type a new player"
                        value={seat.playerName}
                        onChange={(event) => {
                          const player = playersQuery.data?.find(
                            (candidate) =>
                              candidate.name.toLowerCase() ===
                              event.target.value.trim().toLowerCase(),
                          )
                          updateSeat(index, {
                            playerId: player?.id ?? null,
                            playerName: event.target.value,
                            deckId: null,
                            deckName: "",
                            commander: null,
                            partner: null,
                            colorIdentity: "",
                            decklistUrl: "",
                          })
                        }}
                        required
                      />
                    </label>
                    <label className="form-control">
                      <span className="label-text mb-1 text-xs font-medium">Deck (optional)</span>
                      <input
                        className="input input-bordered input-sm w-full"
                        list={`decks-${index}`}
                        placeholder="Choose or type a new deck"
                        value={seat.deckName}
                        onChange={(event) => {
                          const deck = playerDecks.find(
                            (candidate) =>
                              candidate.name.toLowerCase() ===
                              event.target.value.trim().toLowerCase(),
                          )
                          updateSeat(index, {
                            deckId: deck?.id ?? null,
                            deckName: event.target.value,
                            commander: cardSnapshot(
                              deck?.commander_card_id,
                              deck?.commander_name,
                              deck?.color_identity.split("") ?? [],
                            ),
                            partner: cardSnapshot(deck?.partner_card_id, deck?.partner_name),
                            colorIdentity: deck?.color_identity ?? "",
                            decklistUrl: deck?.decklist_url ?? "",
                          })
                        }}
                      />
                      <datalist id={`decks-${index}`}>
                        {playerDecks.map((deck) => (
                          <option key={deck.id} value={deck.name}>
                            {deck.commander_name}
                          </option>
                        ))}
                      </datalist>
                    </label>
                    {seat.deckName && seat.deckId === null && !selectedDeck && (
                      <div className="grid gap-3 sm:col-span-2 sm:grid-cols-2">
                        <DeckFormFields
                          value={seat}
                          onChange={(patch) => updateSeat(index, patch)}
                          onResolvedName={(deckName) => updateSeat(index, { deckName })}
                        />
                      </div>
                    )}
                    <MvpCardField
                      value={seat.mvpCard}
                      onChange={(mvpCard) => updateSeat(index, { mvpCard })}
                    />
                    <label className="form-control">
                      <span className="label-text mb-1 text-xs font-medium">Kills (optional)</span>
                      <input
                        type="number"
                        min="0"
                        max="5"
                        step="1"
                        className="input input-bordered input-sm w-full"
                        value={seat.kills}
                        onChange={(event) => updateSeat(index, { kills: event.target.value })}
                      />
                    </label>
                  </div>
                  {seat.playerName && seat.playerId === null && !player && (
                    <p className="text-info mt-2 text-xs">
                      A new player named “{seat.playerName}” will be created.
                    </p>
                  )}
                </SeatEditor>
              )
            })}
          </div>
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={winnerSeatId === null}
              onChange={(event) =>
                setWinnerSeatId(event.target.checked ? null : (seats[0]?.draftId ?? null))
              }
            />
            <span className="label-text">Game ended in a draw</span>
          </label>
          {error?.fieldErrors("seats").map((message) => (
            <p key={message} className="text-error text-sm">
              {message}
            </p>
          ))}
        </div>
      </section>

      <section className="card border-base-300 bg-base-200 border">
        <div className="card-body grid gap-4 p-4 sm:grid-cols-3 sm:p-6">
          <label className="form-control sm:col-span-3">
            <span className="label-text mb-1 text-sm font-medium">Played at</span>
            <input
              type="datetime-local"
              className="input input-bordered w-full"
              value={playedAt}
              onChange={(event) => setPlayedAt(event.target.value)}
              required
            />
          </label>
          <label className="form-control">
            <span className="label-text mb-1 text-sm font-medium">Turns</span>
            <input
              type="number"
              min="1"
              className="input input-bordered w-full"
              value={turns}
              onChange={(event) => setTurns(event.target.value)}
            />
          </label>
          <label className="form-control">
            <span className="label-text mb-1 text-sm font-medium">Minutes</span>
            <input
              type="number"
              min="1"
              className="input input-bordered w-full"
              value={duration}
              onChange={(event) => setDuration(event.target.value)}
            />
          </label>
          <label className="form-control sm:col-span-3">
            <span className="label-text mb-1 text-sm font-medium">Notes</span>
            <textarea
              className="textarea textarea-bordered min-h-24"
              placeholder="How did the game end?"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
            />
          </label>
        </div>
      </section>

      {mutation.isError && (
        <div role="alert" className="alert alert-error">
          <span>{error?.detail ?? "Could not save the game. Check the highlighted fields."}</span>
        </div>
      )}
      <div className="flex justify-end">
        <button className="btn btn-primary btn-lg w-full sm:w-auto" disabled={mutation.isPending}>
          {mutation.isPending ? "Saving…" : game ? "Save changes" : "Log game"}
        </button>
      </div>
    </form>
  )
}
