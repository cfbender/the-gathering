import type { TableParticipant } from "./use-webcam-room"

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
  const others = participants.filter((seat) => seat.peer_id !== peerId)
  return (
    <div className="w-44 shrink-0 rounded bg-base-100 p-2 text-xs lg:sticky lg:top-0 lg:z-10 lg:w-auto">
      {target ? (
        <>
          <p className="mb-2" role="status">
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
            End reveal
          </button>
          <p className="mt-1 text-white/50">
            Ends if they leave. Put your hand down before ending.
          </p>
        </>
      ) : (
        <label className="grid gap-1">
          <span className="font-semibold">Reveal hand to</span>
          <select
            className="select select-bordered select-xs w-full"
            value=""
            disabled={busy || others.length === 0}
            onChange={(event) => void onChange(event.target.value)}
          >
            <option value="">Choose a player…</option>
            {others.map((seat) => (
              <option key={seat.peer_id} value={seat.peer_id}>
                {seat.player_name}
              </option>
            ))}
          </select>
          <span className="text-white/50">Wait for confirmation before showing your hand.</span>
        </label>
      )}
    </div>
  )
}
