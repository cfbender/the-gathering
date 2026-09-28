import { Play } from "lucide-react"
import { Button } from "@/components/ui/button"
import { awaitingStart } from "./game-timer"
import { isCurrentTurn, type TableView } from "./table-view"

/** Mulligan window between Start match and the first turn's clock. The first player gets a
 * Start button over the stage; everyone else sees who the table is waiting on. */
export function StartOverlay({ view }: { view: TableView }) {
  const { room } = view
  if (!awaitingStart(room.timer?.state)) return null

  if (!room.spectating && isCurrentTurn(view, view.localParticipant)) {
    return (
      <div className="absolute inset-0 z-30 flex items-center justify-center bg-base-100/40">
        <div
          role="dialog"
          aria-label="Start the game"
          className="flex flex-col items-center gap-3 rounded-2xl border border-primary/40 bg-base-100/95 px-8 py-6 text-center shadow-xl"
        >
          <p className="text-sm text-base-content/70">
            You go first. Take your mulligans, then start the clock.
          </p>
          <Button type="button" className="btn-lg px-10 text-lg" onClick={room.beginPlay}>
            <Play className="size-5" /> Start
          </Button>
        </div>
      </div>
    )
  }

  const first = view.seated
    .filter((participant) => isCurrentTurn(view, participant))
    .map((participant) => participant.player_name)
  return (
    <div
      role="status"
      className="pointer-events-none absolute top-20 left-1/2 z-30 w-max max-w-[90%] -translate-x-1/2 rounded-xl border border-primary/40 bg-base-100/95 px-6 py-3 text-center text-sm font-semibold text-base-content shadow-xl"
    >
      Mulligans — waiting for {first.length > 0 ? first.join(" & ") : "the first player"} to start
    </div>
  )
}
