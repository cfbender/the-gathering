import { Eye, EyeOff } from "lucide-react"
import type { TableParticipant } from "./use-webcam-room"

/** Table-tab action: show your camera to one player and hide it from everyone else. */
export function RevealControl({
  participants,
  peerId,
  target,
  busy,
  onChange,
}: {
  participants: TableParticipant[]
  peerId: string
  target: string | null
  busy: boolean
  onChange: (target: string | null) => Promise<void>
}) {
  const others = participants.filter((seat) => seat.peer_id !== peerId && !seat.departed)

  if (target) {
    return (
      <div className="bg-warning/10 grid gap-1.5 rounded-md border border-warning/30 p-2 text-xs">
        <p className="flex items-center gap-1.5 font-semibold" role="status">
          <Eye className="text-warning size-3.5 shrink-0" />
          {busy
            ? "Preparing private video…"
            : `Revealing only to ${others.find((seat) => seat.peer_id === target)?.player_name ?? "departed player"}`}
        </p>
        <button
          type="button"
          className="btn btn-warning btn-xs w-full"
          disabled={busy}
          onClick={() => void onChange(null)}
        >
          <EyeOff className="size-3" /> End reveal
        </button>
        <p className="text-base-content/50 text-[0.65rem]">
          Ends if they leave. Put your hand down before ending.
        </p>
      </div>
    )
  }

  return (
    <label className="grid gap-1 text-xs">
      <span className="sr-only">Reveal hand to</span>
      <select
        aria-label="Reveal hand to"
        className="select select-bordered select-sm w-full text-xs"
        value=""
        disabled={busy || others.length === 0}
        onChange={(event) => void onChange(event.target.value)}
      >
        <option value="">Reveal hand to…</option>
        {others.map((seat) => (
          <option key={seat.peer_id} value={seat.peer_id}>
            {seat.player_name}
          </option>
        ))}
      </select>
    </label>
  )
}
