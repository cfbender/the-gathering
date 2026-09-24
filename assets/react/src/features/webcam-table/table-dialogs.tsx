import { useRef } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { FinishGame } from "./finish-game"
import type { GameTimerState } from "./game-timer"
import { RevealControl } from "./reveal-control"
import { HotkeyHelp } from "./table-hotkeys"
import type { TableView } from "./table-view"

/** Which table dialog is open; only one is at a time. `finish` carries the paused timer the
 * result form records. */
export type TableDialog =
  | { kind: "reveal" }
  | { kind: "help" }
  | { kind: "finish"; timer: GameTimerState }
  | null

export function TableDialogs({
  view,
  dialog,
  onDialogChange,
}: {
  view: TableView
  dialog: TableDialog
  onDialogChange: (dialog: TableDialog) => void
}) {
  const { room } = view
  const playedAt = useRef(new Date())
  const close = (open: boolean) => {
    if (!open) onDialogChange(null)
  }

  return (
    <>
      <Dialog open={dialog?.kind === "reveal"} onOpenChange={close}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Reveal hand</DialogTitle>
            <DialogClose onClose={() => onDialogChange(null)} />
          </DialogHeader>
          <p className="mb-3 text-sm text-base-content/65">
            Wait for confirmation before showing your hand. Only the chosen player receives your
            video.
          </p>
          <RevealControl
            participants={view.seated}
            peerId={room.peerId}
            target={room.revealTo}
            busy={room.revealBusy}
            onChange={room.changeReveal}
          />
        </DialogContent>
      </Dialog>
      <HotkeyHelp open={dialog?.kind === "help"} onOpenChange={close} />
      {dialog?.kind === "finish" && (
        <FinishGame
          mode={room.mode}
          participants={room.participants}
          playedAt={
            dialog.timer.started_at === null ? playedAt.current : new Date(dialog.timer.started_at)
          }
          timer={dialog.timer}
          onOpenChange={close}
        />
      )}
    </>
  )
}
