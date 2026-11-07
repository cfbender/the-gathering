import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { Crown } from "lucide-react"
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
import {
  elapsedMinutes,
  gamePayload,
  lastStanding,
  recordBlocker,
  recordBlockerMessage,
  seatName,
  type TabletopGame,
} from "./tabletop"

type After = "rematch" | "close"

/** Ends a tabletop game. A game every seat of which is a playgroup member in a recorded format
 * goes into history; any other game just ends. Either way the same table can go again. */
export function FinishTabletop({
  game,
  onRematch,
  onClose,
  onOpenChange,
}: {
  game: TabletopGame
  /** Starts another game with the same seats. */
  onRematch: () => void
  /** Clears the table from this device. */
  onClose: () => void
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { toast } = useToast()
  const blocker = recordBlocker(game)
  const [winner, setWinner] = useState<string>(() => {
    const standing = lastStanding(game)
    return standing === null ? "" : String(standing)
  })
  const [duration, setDuration] = useState(() => String(elapsedMinutes(game) ?? ""))
  const [turns, setTurns] = useState(() => (game.turn > 0 ? String(game.turn) : ""))
  const [winCondition, setWinCondition] = useState("")

  const record = useMutation({
    mutationFn: (_after: After) =>
      api<{ data: Game }>("/api/games", {
        method: "POST",
        body: JSON.stringify(
          gamePayload(game, {
            winner: winner === "draw" ? "draw" : Number(winner),
            durationMinutes: duration,
            turns,
            winCondition,
          }),
        ),
      }).then((body) => body.data),
    onSuccess: async (saved, after) => {
      await invalidateGameRelated(queryClient)
      if (after === "rematch") {
        toast({ message: "Game recorded. Rematch ready.", tone: "success" })
        onRematch()
        return
      }
      // Leave the board first: without a game it would send everyone back to setup instead.
      await navigate({ to: "/games/$gameId", params: { gameId: String(saved.id) } })
      onClose()
    },
  })
  const saveError = record.error instanceof ApiError ? record.error : null
  const error = saveError
    ? saveError.detail ||
      Object.keys(saveError.errors)
        .flatMap((field) => saveError.fieldErrors(field))
        .join(" · ")
    : record.error
      ? "Could not record the game. Check your connection and try again."
      : null

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <div>
            <DialogTitle>Game over</DialogTitle>
            <p className="text-base-content/60 mt-1 text-sm">
              {blocker
                ? recordBlockerMessage(blocker)
                : "Confirm the result before it goes into game history."}
            </p>
          </div>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <form
          className="grid gap-5 p-5 sm:grid-cols-2 sm:p-6"
          onSubmit={(event) => {
            event.preventDefault()
            record.mutate("close")
          }}
        >
          {!blocker && (
            <fieldset className="sm:col-span-2">
              <legend className="mb-2 text-sm font-bold">Winner</legend>
              <ToggleGroup
                type="single"
                value={winner}
                onValueChange={setWinner}
                aria-label="Winner"
                className="grid gap-2 sm:grid-cols-2"
              >
                {game.seats.map((seat, index) => (
                  <ToggleGroupItem
                    key={index}
                    value={String(index)}
                    className={cn(
                      "btn h-auto min-h-12 justify-start px-4 py-2",
                      winner === String(index) ? "btn-primary" : "btn-outline",
                    )}
                  >
                    {winner === String(index) && <Crown className="size-4" />}
                    <span className="min-w-0 truncate">{seatName(seat, index)}</span>
                    <span className="ml-auto truncate text-xs font-normal opacity-65">
                      {seat.deck?.name ?? `${seat.life} life`}
                    </span>
                  </ToggleGroupItem>
                ))}
                <ToggleGroupItem
                  value="draw"
                  className={cn(
                    "btn h-auto min-h-12 px-4 py-2",
                    winner === "draw" ? "btn-primary" : "btn-outline",
                  )}
                >
                  Draw
                </ToggleGroupItem>
              </ToggleGroup>
            </fieldset>
          )}

          {!blocker && (
            <>
              <label className="form-control">
                <span className="label-text mb-1">Duration (minutes)</span>
                <input
                  className="input input-bordered w-full"
                  type="number"
                  min="1"
                  step="1"
                  placeholder="Optional"
                  value={duration}
                  onChange={(event) => setDuration(event.target.value)}
                />
              </label>
              <label className="form-control">
                <span className="label-text mb-1">Turns</span>
                <input
                  className="input input-bordered w-full"
                  type="number"
                  min="1"
                  step="1"
                  placeholder="Optional"
                  value={turns}
                  onChange={(event) => setTurns(event.target.value)}
                />
              </label>
              <label className="form-control sm:col-span-2">
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
            </>
          )}

          {error && <div className="alert alert-error sm:col-span-2">{error}</div>}

          <div className="flex flex-wrap justify-end gap-2 sm:col-span-2">
            {blocker ? (
              <>
                <button
                  type="button"
                  className="btn btn-ghost sm:mr-auto"
                  onClick={() => {
                    onClose()
                    void navigate({ to: "/tabletop" })
                  }}
                >
                  New table
                </button>
                <button type="button" className="btn btn-primary" onClick={onRematch}>
                  Rematch
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  className="btn btn-ghost text-error sm:mr-auto"
                  disabled={record.isPending}
                  onClick={onRematch}
                >
                  Rematch without recording
                </button>
                <button
                  type="button"
                  className="btn btn-outline"
                  disabled={!winner || record.isPending}
                  onClick={() => record.mutate("rematch")}
                >
                  Record and rematch
                </button>
                <button className="btn btn-primary" disabled={!winner || record.isPending}>
                  {record.isPending ? "Recording…" : "Record game"}
                </button>
              </>
            )}
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
