import { UserRoundMinus } from "lucide-react"
import { useState } from "react"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import type { TableParticipant } from "./room-types"

/** Shared by the seat roster and spectator list. Permission is supplied by the server. */
export function RemoveParticipant({
  participant,
  onRemove,
}: {
  participant: TableParticipant
  onRemove: (peerId: string) => void
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button
        type="button"
        className="btn btn-ghost btn-xs btn-square shrink-0 text-error"
        aria-label={`Remove ${participant.player_name} from table`}
        title={`Remove ${participant.player_name} from table`}
        onClick={() => setOpen(true)}
      >
        <UserRoundMinus className="size-3.5" />
      </button>
      <ConfirmDialog
        open={open}
        onOpenChange={setOpen}
        title={`Remove ${participant.player_name} from the table?`}
        confirmLabel="Remove from table"
        destructive
        onConfirm={() => onRemove(participant.peer_id)}
      >
        They will be disconnected and cannot rejoin this table, including rematches.
        {!participant.spectator &&
          " If the game has started, their seat stays in the results as eliminated."}
      </ConfirmDialog>
    </>
  )
}
