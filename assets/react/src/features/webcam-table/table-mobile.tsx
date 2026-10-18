import { LayoutDashboard, Menu, Minus, Plus, SkipForward, VideoOff } from "lucide-react"
import { cn } from "@/lib/cn"
import { boardFor, isCurrentTurn, isLocal, seatLife, togglePin, type TableView } from "./table-view"

/** Phones replace the camera rail with one chip per seat: name, life, and turn at a glance,
 * and a tap shows (or releases) that board, like clicking a rail camera. */
export function MobileSeatStrip({ view }: { view: TableView }) {
  return (
    <nav
      className="flex min-w-0 gap-1.5 overflow-x-auto bg-base-100 p-1.5 lg:hidden max-lg:landscape:col-start-1 max-lg:landscape:row-start-1"
      aria-label="Players"
    >
      {view.seated.map((participant) => {
        const shown = !view.showGrid && participant.peer_id === view.activeParticipant.peer_id
        const turn = isCurrentTurn(view, participant) && !participant.eliminated
        const life = seatLife(view, participant)
        return (
          <button
            key={participant.peer_id}
            type="button"
            className={cn(
              "flex h-11 max-w-36 shrink-0 items-center gap-2 rounded-lg border-2 px-2.5 text-left",
              shown
                ? "border-primary bg-primary text-primary-content"
                : "border-base-content/15 bg-base-200 text-base-content",
              turn && !shown && "border-accent",
              participant.eliminated && "opacity-50",
            )}
            aria-pressed={shown}
            aria-label={`Show ${participant.player_name}'s board, ${life} life${turn ? ", current turn" : ""}`}
            onClick={() => togglePin(view, participant)}
          >
            <span className="min-w-0">
              <span
                className={cn(
                  "block truncate text-xs leading-tight font-semibold",
                  participant.eliminated && "line-through",
                )}
              >
                {participant.player_name}
                {isLocal(view, participant) && <span className="opacity-70"> (you)</span>}
              </span>
              {turn && (
                <span
                  className={cn(
                    "block text-[0.6rem] leading-tight font-bold",
                    !shown && "text-accent",
                  )}
                >
                  ● Turn
                </span>
              )}
            </span>
            {participant.camera_off && !boardFor(view, participant) && (
              <VideoOff className="size-3.5 shrink-0 text-error" aria-label="Camera off" />
            )}
            <span className="text-lg font-black tabular-nums">{life}</span>
          </button>
        )
      })}
    </nav>
  )
}

/** The phone's thumb row: your life, Pass turn, and the table panel. */
export function MobileActionBar({
  view,
  panelOpen,
  onOpenPanel,
}: {
  view: TableView
  panelOpen: boolean
  onOpenPanel: () => void
}) {
  const { room, localParticipant } = view
  const seated = !room.spectating
  const started = room.timer?.state.started_at != null
  const myTurn = isCurrentTurn(view, localParticipant)
  return (
    <div className="bg-base-100 text-base-content lg:hidden max-lg:landscape:col-start-2 max-lg:landscape:row-start-1 max-lg:portrait:border-t max-lg:portrait:border-base-content/10">
      {room.error && !panelOpen && (
        <p role="alert" className="bg-error/20 px-3 py-1.5 text-xs text-error">
          {room.error}
        </p>
      )}
      <div className="flex items-center gap-1.5 p-1.5">
        {seated && (
          <div
            className="flex items-center rounded-lg border border-base-content/15"
            aria-label="Your life"
          >
            <button
              type="button"
              className="btn btn-ghost btn-square h-11 min-h-11 w-10"
              aria-label="Lose 1 life"
              onClick={() => room.changeLife(-1)}
            >
              <Minus className="size-4" />
            </button>
            <span className="min-w-8 text-center text-xl font-black tabular-nums">
              {seatLife(view, localParticipant)}
            </span>
            <button
              type="button"
              className="btn btn-ghost btn-square h-11 min-h-11 w-10"
              aria-label="Gain 1 life"
              onClick={() => room.changeLife(1)}
            >
              <Plus className="size-4" />
            </button>
          </div>
        )}
        {seated && started && (
          <button
            type="button"
            className={cn(
              "btn h-11 min-h-11 flex-1 gap-1.5 whitespace-nowrap max-lg:landscape:flex-none",
              myTurn ? "btn-primary" : "btn-outline",
            )}
            onClick={room.passTurn}
            disabled={room.turns.active_player_id === null}
          >
            <SkipForward className="size-4" /> Pass turn
          </button>
        )}
        {view.virtualBoard.active && (
          <button
            type="button"
            className="btn btn-ghost btn-square h-11 min-h-11 w-11"
            aria-label="Open my board"
            onClick={view.openBoard}
          >
            <LayoutDashboard className="size-5" />
          </button>
        )}
        <button
          type="button"
          className="btn btn-ghost btn-square ml-auto h-11 min-h-11 w-11"
          aria-label="Open table panel"
          aria-expanded={panelOpen}
          onClick={onOpenPanel}
        >
          <Menu className="size-5" />
        </button>
      </div>
    </div>
  )
}
