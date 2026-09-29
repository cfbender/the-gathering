import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useToast } from "@/components/ui/toast"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { invalidateGameRelated, WIN_CONDITIONS, type Game } from "@/features/games/games"
import { api, ApiError } from "@/lib/api"
import { cn } from "@/lib/cn"
import { buildGamePayload, suggestedWinner } from "./game-result"
import { durationMinutes, type GameTimerState } from "./game-timer"
import type { TableParticipant } from "./use-webcam-room"
import type { GameFormat } from "@/features/games/game-format"
import { teams } from "./game-modes"
import { highestTurn, type TurnState } from "./turns"

interface Props {
  mode?: GameFormat
  /** Seated players in turn order; seats are recorded 1..n in this order. */
  participants: TableParticipant[]
  playedAt: Date
  timer: GameTimerState
  /** The table's turn counts; the highest one prefills the recorded turns. */
  turns: TurnState
  /** Closes the table for every seat; resolves whether the server accepted. */
  onEndTable: () => Promise<boolean>
  /** Resets the room to a fresh lobby with the same seats; resolves whether the server accepted. */
  onRematch: () => Promise<boolean>
  onOpenChange: (open: boolean) => void
}

/** What happens to the room once the game is recorded or skipped. */
type AfterGame = "close" | "rematch"

/** End game → result form → POST /api/games, so the table lands in normal history. Either way
 * (recorded or not) the table then either closes for everyone, sending this seat to the game
 * or the games list, or resets to a fresh lobby for a rematch without navigating anywhere. */
export function FinishGame({
  participants,
  playedAt,
  timer,
  turns: turnState,
  onEndTable,
  onRematch,
  onOpenChange,
  mode = "commander",
}: Props) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { toast } = useToast()
  const [after, setAfter] = useState<AfterGame>("close")
  const rematch = after === "rematch"
  const [winner, setWinner] = useState(() => suggestedWinner(participants, mode))
  const choices =
    mode === "two_headed_giant"
      ? teams(participants).map((team, index) => ({
          ...team[0]!,
          player_name: `Team ${index + 1}: ${team.map((seat) => seat.player_name).join(" + ")}`,
          deck_name: team
            .map((seat) => seat.deck_name)
            .filter(Boolean)
            .join(" / "),
          eliminated: team.every((seat) => seat.eliminated),
        }))
      : participants
  const [duration, setDuration] = useState(() => durationMinutes(timer))
  const [turns, setTurns] = useState(() => {
    const highest = highestTurn(turnState)
    return highest > 0 ? String(highest) : ""
  })
  const [winCondition, setWinCondition] = useState("")
  const [notes, setNotes] = useState("")
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const mutation = useMutation({
    mutationFn: (_after: AfterGame) =>
      api<{ data: Game }>("/api/games", {
        method: "POST",
        body: JSON.stringify(
          buildGamePayload(
            participants,
            {
              playedAt,
              winner,
              duration,
              turns,
              winCondition,
              notes,
            },
            mode,
          ),
        ),
      }).then((body) => body.data),
    onSuccess: async (game, next) => {
      if (next === "rematch") {
        // The game is saved either way, so the form closes rather than offer to record it twice.
        const [, started] = await Promise.all([invalidateGameRelated(queryClient), onRematch()])
        toast(
          started
            ? { message: "Game recorded. The rematch is ready in the same room.", tone: "success" }
            : {
                message:
                  "Game recorded, but the rematch did not start. Try End game → Rematch without recording.",
              },
        )
        onOpenChange(false)
        return
      }
      // The game is saved; a table that fails to close is still pruned once everyone leaves.
      await Promise.all([invalidateGameRelated(queryClient), onEndTable()])
      void navigate({ to: "/games/$gameId", params: { gameId: String(game.id) } })
    },
  })
  const discard = useMutation({
    mutationFn: (next: AfterGame) => (next === "rematch" ? onRematch() : onEndTable()),
    onSuccess: (done, next) => {
      if (!done) return
      if (next === "close") {
        void navigate({ to: "/games" })
        return
      }
      toast({ message: "The rematch is ready in the same room.", tone: "success" })
      onOpenChange(false)
    },
  })
  const busy = mutation.isPending || discard.isPending
  const error =
    mutation.error instanceof ApiError
      ? mutation.error.detail
      : discard.data === false
        ? discard.variables === "rematch"
          ? "Could not start the rematch. Check your connection to the table and try again."
          : "Could not end the game. Check your connection to the table and try again."
        : null

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <div>
            <DialogTitle>Record game result</DialogTitle>
            <p className="text-base-content/60 mt-1 text-sm">
              Confirm the outcome before adding this game to history.
            </p>
          </div>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <form
          className="grid gap-5 p-5 sm:p-6 md:grid-cols-2"
          onSubmit={(event) => {
            event.preventDefault()
            mutation.mutate(after)
          }}
        >
          <fieldset className="md:col-span-2">
            <legend className="mb-2 text-sm font-bold">Winner</legend>
            <ToggleGroup
              type="single"
              value={winner}
              onValueChange={setWinner}
              aria-label="Winner"
              className="grid gap-2 sm:grid-cols-2"
            >
              {choices.map((participant, index) => (
                <ToggleGroupItem
                  key={participant.peer_id}
                  value={participant.peer_id}
                  className={cn(
                    "btn h-auto min-h-12 justify-start px-4 py-3",
                    winner === participant.peer_id ? "btn-primary" : "btn-outline",
                  )}
                >
                  <span className="mr-2 opacity-60 tabular-nums">{index + 1}.</span>
                  {participant.player_name}
                  {participant.eliminated && (
                    <span className="ml-2 text-xs opacity-60">Eliminated</span>
                  )}
                  <span className="ml-auto text-xs opacity-65">
                    {participant.deck_name ?? "No deck"}
                  </span>
                </ToggleGroupItem>
              ))}
              <ToggleGroupItem
                value="draw"
                className={cn(
                  "btn h-auto min-h-12 px-4 py-3",
                  winner === "draw" ? "btn-primary" : "btn-outline",
                )}
              >
                Draw
              </ToggleGroupItem>
            </ToggleGroup>
          </fieldset>
          {participants.some((participant) => participant.eliminated) && (
            <p className="text-base-content/60 text-xs md:col-span-2">
              The last {mode === "two_headed_giant" ? "team" : "player"} still in suggests the
              winner; everyone else records a loss. You can correct the winner or choose a draw for
              the whole table.
            </p>
          )}
          <label className="form-control">
            <span className="label-text mb-1">Duration (minutes)</span>
            <input
              className="input input-bordered"
              type="number"
              min="1"
              step="1"
              placeholder="Optional"
              value={duration}
              onChange={(event) => setDuration(event.target.value)}
            />
            <span className="text-base-content/50 mt-1 text-xs">
              {timer.started_at === null
                ? "Timer was not started; enter a duration if known."
                : "Shared timer, excluding pauses. Rounded to the nearest minute (minimum 1)."}
            </span>
          </label>
          <label className="form-control">
            <span className="label-text mb-1">Turns</span>
            <input
              className="input input-bordered"
              type="number"
              min="1"
              placeholder="Optional"
              value={turns}
              onChange={(event) => setTurns(event.target.value)}
            />
          </label>
          <label className="form-control md:col-span-2">
            <span className="label-text mb-1">Win condition</span>
            <select
              className="select select-bordered w-full"
              value={winCondition}
              onChange={(event) => setWinCondition(event.target.value)}
            >
              <option value="">Not recorded</option>
              {WIN_CONDITIONS.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="form-control md:col-span-2">
            <span className="label-text mb-1">Notes</span>
            <textarea
              className="textarea textarea-bordered"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
            />
          </label>
          <fieldset className="md:col-span-2">
            <legend className="mb-2 text-sm font-bold">After this game</legend>
            <ToggleGroup
              type="single"
              value={after}
              onValueChange={(value) => {
                if (!value) return
                setAfter(value as AfterGame)
                discard.reset()
              }}
              aria-label="After this game"
              className="grid gap-2 sm:grid-cols-2"
            >
              {(
                [
                  ["close", "Close the table", "Everyone leaves the room"],
                  ["rematch", "End and rematch", "Same room and seats, back to setup"],
                ] as const
              ).map(([value, label, hint]) => (
                <ToggleGroupItem
                  key={value}
                  value={value}
                  disabled={busy}
                  className={cn(
                    "btn h-auto min-h-12 flex-col items-start gap-0.5 px-4 py-2 text-left",
                    after === value ? "btn-primary" : "btn-outline",
                  )}
                >
                  {label}
                  <span className="text-xs font-normal opacity-65">{hint}</span>
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </fieldset>
          {error && <div className="alert alert-error md:col-span-2">{error}</div>}
          {participants.length < 2 && (
            <p className="text-base-content/60 text-sm md:col-span-2">
              At least two players must be in the room to record a game.
            </p>
          )}
          {confirmDiscard ? (
            <div
              role="alert"
              className="rounded-box border-warning/50 bg-warning/10 flex flex-wrap items-center justify-between gap-3 border p-3 md:col-span-2"
            >
              <span className="text-sm">
                {rematch
                  ? "Start a rematch for everyone without adding this game to history?"
                  : "End the game for everyone without adding it to history?"}
              </span>
              <div className="ml-auto flex gap-2">
                <button
                  type="button"
                  className="btn btn-sm btn-ghost"
                  onClick={() => setConfirmDiscard(false)}
                  disabled={discard.isPending}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className="btn btn-sm btn-error"
                  onClick={() => discard.mutate(after)}
                  disabled={busy}
                >
                  {discard.isPending
                    ? rematch
                      ? "Starting…"
                      : "Ending…"
                    : rematch
                      ? "Rematch without recording"
                      : "End without recording"}
                </button>
              </div>
            </div>
          ) : (
            <div className="flex flex-wrap justify-end gap-2 md:col-span-2">
              <button
                type="button"
                className="btn btn-ghost text-error sm:mr-auto"
                onClick={() => setConfirmDiscard(true)}
                disabled={busy}
              >
                {rematch ? "Rematch without recording" : "End without recording"}
              </button>
              <button type="button" className="btn btn-ghost" onClick={() => onOpenChange(false)}>
                Back to game
              </button>
              <button
                className="btn btn-primary min-w-36"
                disabled={participants.length < 2 || !winner || busy}
              >
                {mutation.isPending
                  ? "Recording…"
                  : rematch
                    ? "Record and rematch"
                    : "Record result"}
              </button>
            </div>
          )}
          {timer.started_at !== null && (
            <p className="text-base-content/50 text-xs md:col-span-2">
              The table timer is paused. If you go back, use Resume timer to keep playing.
            </p>
          )}
        </form>
      </DialogContent>
    </Dialog>
  )
}
