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
  type DiscordResultDraft,
} from "@/features/games/use-game-draft"
import { MvpCardField } from "@/components/mvp-card-field"
import { api, ApiError } from "@/lib/api"
import { cardSnapshot } from "@/lib/cards"
import {
  getPlayers,
  invalidateGameRelated,
  WIN_CONDITIONS,
  type Game,
  type PlayerSummary,
} from "@/features/games/games"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import { SeatEditor } from "@/features/games/seat-editor"
import {
  SELECT_NONE_VALUE,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

interface GameFormProps {
  game?: Game
  discordDraft?: DiscordResultDraft
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

export function GameForm({ game, discordDraft }: GameFormProps) {
  return (
    <GameFormDraft
      key={game?.id ?? discordDraft?.id ?? "new-game"}
      game={game}
      discordDraft={discordDraft}
    />
  )
}

function GameFormDraft({ game, discordDraft }: GameFormProps) {
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
    winCondition,
    setWinCondition,
    notes,
    setNotes,
    updateSeat,
  } = useGameDraft(game, discordDraft)
  const hasResult = winnerSeatId === null || seats.some((seat) => seat.draftId === winnerSeatId)

  const mutation = useMutation({
    mutationFn: async () => {
      const results = resultsForSeats(seats, winnerSeatId)
      const payloadSeats = []
      const knownPlayers = [...(playersQuery.data ?? [])]
      const knownDecks = [...(decksQuery.data ?? [])]

      for (const [index, draft] of seats.entries()) {
        if (discordDraft) {
          payloadSeats.push({
            discord_id: draft.discordId,
            deck_id: draft.deckId,
            deck:
              draft.deckId === null && draft.deckName.trim()
                ? {
                    name: draft.deckName.trim(),
                    commander_card_id: draft.commander?.catalog_id ?? null,
                    commander_name: draft.commander?.name ?? null,
                    partner_card_id: draft.partner?.catalog_id ?? null,
                    partner_name: draft.partner?.name ?? null,
                    color_identity: draft.colorIdentity,
                    decklist_url: draft.decklistUrl.trim() || null,
                  }
                : null,
            result: results[index],
            kills: draft.kills === "" ? null : Number(draft.kills),
            mvp_card_id: draft.mvpCard?.catalog_id ?? null,
            mvp_card_name: draft.mvpCard?.name ?? null,
          })
          continue
        }
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
          win_condition: winCondition || null,
          notes: notes.trim() || null,
          seats: payloadSeats,
        },
      }
      const path = discordDraft
        ? `/api/discord/result-drafts/${discordDraft.id}`
        : game
          ? `/api/games/${game.id}`
          : "/api/games"
      return api<{ data: Game }>(path, {
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
    if (hasResult) mutation.mutate()
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
              disabled={Boolean(discordDraft) || seats.length >= 6}
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
                  canRemove={!discordDraft}
                >
                  <div className="grid gap-3 sm:grid-cols-2">
                    <label className="form-control">
                      <span className="label-text mb-1 text-xs font-medium">Player</span>
                      <input
                        className="input input-bordered input-sm w-full"
                        list="player-names"
                        placeholder="Choose or type a new player"
                        value={seat.playerName}
                        disabled={Boolean(discordDraft)}
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
                  {!discordDraft && seat.playerName && seat.playerId === null && !player && (
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

      {discordDraft && (
        <div role="status" className="alert alert-info">
          <span>
            SpellBot game {discordDraft.external_id}. Nothing has been saved yet; review the details
            and log the game when ready.
          </span>
        </div>
      )}

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
          <div className="form-control sm:col-span-3">
            <span className="label-text mb-1 text-sm font-medium">Win condition</span>
            <Select
              value={winCondition || SELECT_NONE_VALUE}
              onValueChange={(value) => setWinCondition(value === SELECT_NONE_VALUE ? "" : value)}
            >
              <SelectTrigger aria-label="Win condition">
                <SelectValue placeholder="Not recorded" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={SELECT_NONE_VALUE}>Not recorded</SelectItem>
                {WIN_CONDITIONS.map(([value, label]) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <label className="flex min-w-0 flex-col gap-2 sm:col-span-3">
            <span className="text-sm font-semibold">Notes</span>
            <textarea
              className="textarea textarea-bordered min-h-32 w-full resize-y text-base leading-relaxed"
              placeholder="Memorable plays, turning points, or how the game ended…"
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
      {!hasResult && <p role="status">Choose a winner or mark the game as a draw before saving.</p>}
      <div className="flex justify-end">
        <button
          className="btn btn-primary btn-lg w-full sm:w-auto"
          disabled={mutation.isPending || !hasResult}
        >
          {mutation.isPending ? "Saving…" : game ? "Save changes" : "Log game"}
        </button>
      </div>
    </form>
  )
}
