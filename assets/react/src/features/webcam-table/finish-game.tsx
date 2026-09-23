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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { invalidateGameRelated, WIN_CONDITIONS, type Game } from "@/features/games/games"
import { api, ApiError } from "@/lib/api"
import { cn } from "@/lib/cn"
import { buildGamePayload } from "./game-result"
import { durationMinutes, type GameTimerState } from "./game-timer"
import type { TableParticipant } from "./use-webcam-room"

interface Props {
  /** Seated players in turn order; seats are recorded 1..n in this order. */
  participants: TableParticipant[]
  playedAt: Date
  timer: GameTimerState
  onOpenChange: (open: boolean) => void
}

/** End game → result form → POST /api/games, so the table lands in normal history. */
export function FinishGame({ participants, playedAt, timer, onOpenChange }: Props) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [winner, setWinner] = useState("")
  const [duration, setDuration] = useState(() => durationMinutes(timer))
  const [turns, setTurns] = useState("")
  const [winCondition, setWinCondition] = useState("")
  const [notes, setNotes] = useState("")
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: Game }>("/api/games", {
        method: "POST",
        body: JSON.stringify(
          buildGamePayload(participants, {
            playedAt,
            winner,
            duration,
            turns,
            winCondition,
            notes,
          }),
        ),
      }).then((body) => body.data),
    onSuccess: async (game) => {
      await invalidateGameRelated(queryClient)
      void navigate({ to: "/games/$gameId", params: { gameId: String(game.id) } })
    },
  })
  const error = mutation.error instanceof ApiError ? mutation.error.detail : null

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
            mutation.mutate()
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
              {participants.map((participant, index) => (
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
          {error && <div className="alert alert-error md:col-span-2">{error}</div>}
          {participants.length < 2 && (
            <p className="text-base-content/60 text-sm md:col-span-2">
              At least two players must be in the room to record a game.
            </p>
          )}
          <div className="flex justify-end gap-2 md:col-span-2">
            <button type="button" className="btn btn-ghost" onClick={() => onOpenChange(false)}>
              Back to game
            </button>
            <button
              className="btn btn-primary min-w-36"
              disabled={participants.length < 2 || !winner || mutation.isPending}
            >
              {mutation.isPending ? "Recording…" : "Record result"}
            </button>
          </div>
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
