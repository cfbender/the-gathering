import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { ArrowDown, ArrowUp, Plus, Trash2, Trophy } from "lucide-react"
import { useEffect, useState, type FormEvent } from "react"
import { DeckFormFields } from "@/components/deck-form-fields"
import { blankSeat, moveSeat, resultsForSeats, type DraftSeat } from "@/components/game-form-logic"
import { MvpCardField } from "@/components/mvp-card-field"
import { api, ApiError } from "@/lib/api"
import { cardSnapshot } from "@/lib/cards"
import { getDecks, getPlayers, type Deck, type Game, type Player } from "@/lib/games"

interface GameFormProps {
  game?: Game
}

function localDateTime(value?: string) {
  const date = value ? new Date(value) : new Date()
  const offset = date.getTimezoneOffset() * 60_000
  return new Date(date.getTime() - offset).toISOString().slice(0, 16)
}

function draftsFromGame(game: Game): DraftSeat[] {
  return game.seats.map((seat) => ({
    id: seat.id,
    playerName: seat.player.name,
    deckName: seat.deck?.name ?? "",
    commander: cardSnapshot(
      seat.deck?.commander_card_id,
      seat.deck?.commander_name,
      seat.deck?.color_identity.split("") ?? [],
    ),
    partner: cardSnapshot(seat.deck?.partner_card_id, seat.deck?.partner_name),
    colorIdentity: seat.deck?.color_identity ?? "",
    decklistUrl: seat.deck?.decklist_url ?? "",
    mvpCard: cardSnapshot(seat.mvp_card_id, seat.mvp_card_name),
  }))
}

async function ensurePlayer(name: string, players: Player[]) {
  const existing = players.find((player) => player.name.toLowerCase() === name.trim().toLowerCase())
  if (existing) return existing
  return api<{ data: Player }>("/api/players", {
    method: "POST",
    body: JSON.stringify({ player: { name: name.trim() } }),
  }).then((body) => body.data)
}

async function ensureDeck(draft: DraftSeat, player: Player, decks: Deck[]) {
  if (!draft.deckName.trim()) return null
  const existing = decks.find(
    (deck) =>
      deck.player_id === player.id &&
      deck.name.toLowerCase() === draft.deckName.trim().toLowerCase(),
  )
  if (existing) return existing
  return api<{ data: Deck }>("/api/decks", {
    method: "POST",
    body: JSON.stringify({
      deck: {
        player_id: player.id,
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
}

export function GameForm({ game }: GameFormProps) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const playersQuery = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const decksQuery = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  const [playedAt, setPlayedAt] = useState(() => localDateTime(game?.played_at))
  const [seats, setSeats] = useState<DraftSeat[]>(() =>
    game ? draftsFromGame(game) : [blankSeat(), blankSeat()],
  )
  const [winnerIndex, setWinnerIndex] = useState<number | null>(() => {
    if (!game) return 0
    const index = game.seats.findIndex((seat) => seat.result === "win")
    return index < 0 ? null : index
  })
  const [turns, setTurns] = useState(game?.turns?.toString() ?? "")
  const [duration, setDuration] = useState(game?.duration_minutes?.toString() ?? "")
  const [notes, setNotes] = useState(game?.notes ?? "")

  useEffect(() => {
    if (!game) return
    setPlayedAt(localDateTime(game.played_at))
    setSeats(draftsFromGame(game))
    const winner = game.seats.findIndex((seat) => seat.result === "win")
    setWinnerIndex(winner < 0 ? null : winner)
  }, [game])

  const mutation = useMutation({
    mutationFn: async () => {
      const knownPlayers = [...(playersQuery.data ?? [])]
      const knownDecks = [...(decksQuery.data ?? [])]
      const results = resultsForSeats(seats.length, winnerIndex)
      const payloadSeats = []

      for (const [index, draft] of seats.entries()) {
        const player = await ensurePlayer(draft.playerName, knownPlayers)
        if (!knownPlayers.some((candidate) => candidate.id === player.id)) knownPlayers.push(player)
        const deck = await ensureDeck(draft, player, knownDecks)
        if (deck && !knownDecks.some((candidate) => candidate.id === deck.id)) knownDecks.push(deck)
        payloadSeats.push({
          id: draft.id,
          player_id: player.id,
          deck_id: deck?.id ?? null,
          seat: index + 1,
          result: results[index],
          mvp_card_id: draft.mvpCard?.catalog_id,
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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["games"] }),
        queryClient.invalidateQueries({ queryKey: ["players"] }),
        queryClient.invalidateQueries({ queryKey: ["decks"] }),
      ])
      void navigate({ to: "/games/$gameId", params: { gameId: String(saved.id) } })
    },
  })

  function updateSeat(index: number, patch: Partial<DraftSeat>) {
    setSeats((current) =>
      current.map((seat, seatIndex) => (seatIndex === index ? { ...seat, ...patch } : seat)),
    )
  }

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
              const player = playersQuery.data?.find(
                (candidate) =>
                  candidate.name.toLowerCase() === seat.playerName.trim().toLowerCase(),
              )
              const playerDecks =
                decksQuery.data?.filter((deck) => deck.player_id === player?.id) ?? []
              const selectedDeck = playerDecks.find(
                (deck) => deck.name.toLowerCase() === seat.deckName.trim().toLowerCase(),
              )
              return (
                <article
                  key={seat.id ?? `new-${index}`}
                  className="border-base-300 bg-base-100 rounded-box border p-4"
                >
                  <div className="mb-3 flex items-center gap-2">
                    <span className="bg-neutral text-neutral-content grid size-7 place-items-center rounded-full text-xs font-bold">
                      {index + 1}
                    </span>
                    <label className="flex flex-1 items-center gap-2 font-semibold">
                      <input
                        type="radio"
                        name="winner"
                        className="radio radio-success radio-sm"
                        checked={winnerIndex === index}
                        onChange={() => setWinnerIndex(index)}
                        aria-label={`${seat.playerName || `Seat ${index + 1}`} won`}
                      />
                      <Trophy className="text-success size-4" /> Winner
                    </label>
                    <button
                      type="button"
                      className="btn btn-square btn-ghost btn-xs"
                      disabled={index === 0}
                      onClick={() => setSeats((value) => moveSeat(value, index, -1))}
                      aria-label="Move seat up"
                    >
                      <ArrowUp className="size-4" />
                    </button>
                    <button
                      type="button"
                      className="btn btn-square btn-ghost btn-xs"
                      disabled={index === seats.length - 1}
                      onClick={() => setSeats((value) => moveSeat(value, index, 1))}
                      aria-label="Move seat down"
                    >
                      <ArrowDown className="size-4" />
                    </button>
                    <button
                      type="button"
                      className="btn btn-square btn-ghost btn-xs text-error"
                      disabled={seats.length <= 2}
                      onClick={() =>
                        setSeats((value) => value.filter((_, seatIndex) => seatIndex !== index))
                      }
                      aria-label="Remove seat"
                    >
                      <Trash2 className="size-4" />
                    </button>
                  </div>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <label className="form-control">
                      <span className="label-text mb-1 text-xs font-medium">Player</span>
                      <input
                        className="input input-bordered input-sm w-full"
                        list="player-names"
                        placeholder="Choose or type a new player"
                        value={seat.playerName}
                        onChange={(event) =>
                          updateSeat(index, {
                            playerName: event.target.value,
                            deckName: "",
                            commander: null,
                            partner: null,
                            colorIdentity: "",
                            decklistUrl: "",
                          })
                        }
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
                            (candidate) => candidate.name === event.target.value,
                          )
                          updateSeat(index, {
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
                    {seat.deckName && !selectedDeck && (
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
                  </div>
                  {seat.playerName && !player && (
                    <p className="text-info mt-2 text-xs">
                      A new player named “{seat.playerName}” will be created.
                    </p>
                  )}
                </article>
              )
            })}
          </div>
          <label className="label cursor-pointer justify-start gap-3">
            <input
              type="checkbox"
              className="checkbox checkbox-sm"
              checked={winnerIndex === null}
              onChange={(event) => setWinnerIndex(event.target.checked ? null : 0)}
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
