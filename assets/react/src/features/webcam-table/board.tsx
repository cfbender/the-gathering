import { Crown, Pin, PinOff, UserPlus, Video, VideoOff } from "lucide-react"
import type { MouseEvent, ReactNode } from "react"
import { cn } from "@/lib/cn"
import { describeConnection, type TableParticipant } from "./use-webcam-room"

export function StreamVideo({ stream, className }: { stream: MediaStream; className?: string }) {
  return (
    <video
      ref={(video) => {
        if (video && video.srcObject !== stream) video.srcObject = stream
      }}
      className={cn("h-full w-full", className)}
      autoPlay
      playsInline
      // The table is video-only. Muting also permits spectator autoplay without
      // a prior camera grant or interaction with the page.
      muted
    />
  )
}

function VideoPlaceholder({ label, compact }: { label: string; compact?: boolean }) {
  return (
    <div className="grid h-full w-full place-items-center bg-black text-white/40">
      <div className={cn("text-center", compact ? "text-[0.65rem]" : "text-sm")}>
        <Video className={cn("mx-auto mb-1", compact ? "size-4" : "size-7")} />
        {label}
      </div>
    </div>
  )
}

function CameraOffOverlay({ compact }: { compact?: boolean }) {
  return (
    <div className="pointer-events-none absolute inset-0 grid place-items-center bg-black/70 text-white/70">
      <div className={cn("text-center", compact ? "text-[0.65rem]" : "text-sm")}>
        <VideoOff className={cn("mx-auto mb-1", compact ? "size-4" : "size-7")} />
        Camera off
      </div>
    </div>
  )
}

function EliminatedOverlay({ compact = false }: { compact?: boolean }) {
  return (
    <div className="pointer-events-none absolute inset-0 bg-black/50">
      <span
        className={cn(
          "absolute rounded border border-white/25 bg-zinc-950/90 px-2 py-1 text-xs font-bold tracking-wide text-white",
          compact ? "right-1 bottom-1" : "top-2 left-1/2 -translate-x-1/2",
        )}
        aria-label="Eliminated"
      >
        {compact ? "Out" : "Eliminated"}
      </span>
    </div>
  )
}

function CurrentTurnBadge({ compact = false }: { compact?: boolean }) {
  return (
    <span
      className={cn(
        "pointer-events-none absolute rounded border border-amber-300/50 bg-zinc-950/90 px-2 py-0.5 text-[0.65rem] font-bold text-amber-200",
        compact ? "right-1 bottom-1" : "bottom-2 left-2",
      )}
      aria-label="Current turn"
    >
      {compact ? "● Turn" : "● Current turn"}
    </span>
  )
}

/** Maps a click on a `object-contain` video to normalized source coordinates. */
export function capturePoint(event: MouseEvent<HTMLElement>, flipped = false) {
  const video = event.currentTarget.querySelector("video")
  if (!video || !video.videoWidth || !video.videoHeight) return null

  const bounds = event.currentTarget.getBoundingClientRect()
  const sourceRatio = video.videoWidth / video.videoHeight
  const boundsRatio = bounds.width / bounds.height
  const renderedWidth = sourceRatio > boundsRatio ? bounds.width : bounds.height * sourceRatio
  const renderedHeight = sourceRatio > boundsRatio ? bounds.width / sourceRatio : bounds.height
  const left = bounds.left + (bounds.width - renderedWidth) / 2
  const top = bounds.top + (bounds.height - renderedHeight) / 2
  const y = Math.max(0, Math.min(1, (event.clientY - top) / renderedHeight))

  return {
    x: Math.max(0, Math.min(1, (event.clientX - left) / renderedWidth)),
    y: flipped ? 1 - y : y,
  }
}

export function ActiveBoard({
  participant,
  unattackable = false,
  monarch,
  stream,
  local,
  flipped = false,
  currentTurn = false,
  connectionState,
  hiddenLabel,
  revealBadge,
  pinned,
  onTogglePin,
  onInspect,
  lifeControl,
}: {
  participant: TableParticipant
  unattackable?: boolean
  monarch?: boolean
  stream?: MediaStream
  local: boolean
  flipped?: boolean
  currentTurn?: boolean
  connectionState?: RTCPeerConnectionState
  hiddenLabel?: string
  revealBadge?: string
  pinned: boolean
  onTogglePin: () => void
  onInspect: (event: MouseEvent<HTMLButtonElement>) => void
  lifeControl: ReactNode
}) {
  return (
    <div className="relative h-full w-full overflow-hidden bg-black">
      <button
        type="button"
        className="group relative block h-full w-full cursor-crosshair text-left"
        onClick={onInspect}
        disabled={!!hiddenLabel || participant.camera_off}
        aria-label={`Inspect ${participant.player_name}'s board`}
      >
        {hiddenLabel ? (
          <VideoPlaceholder label={hiddenLabel} />
        ) : stream ? (
          <StreamVideo
            stream={stream}
            className={cn("object-contain", flipped && "-scale-y-100")}
          />
        ) : (
          <VideoPlaceholder
            label={
              participant.departed
                ? "Left table"
                : local
                  ? "Starting camera…"
                  : describeConnection(connectionState)
            }
          />
        )}
        {participant.camera_off && <CameraOffOverlay />}
        {revealBadge && (
          <span className="absolute top-28 left-2 rounded bg-primary px-2 py-1 text-xs text-primary-content">
            {revealBadge}
          </span>
        )}
        <span className="pointer-events-none absolute bottom-9 left-1/2 -translate-x-1/2 rounded-full bg-black/75 px-3 py-1 text-xs text-white opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
          Click a card to identify it · Shift+click to choose
        </span>
      </button>
      {participant.eliminated && <EliminatedOverlay />}
      {unattackable && (
        <span className="pointer-events-none absolute bottom-10 left-2 rounded bg-base-100/95 px-2 py-1 text-xs text-warning">
          Can't attack yet
        </span>
      )}
      {currentTurn && !participant.eliminated && <CurrentTurnBadge />}
      {lifeControl}
      {monarch && (
        <span
          className="pointer-events-none absolute top-2 left-28 rounded bg-black/80 p-2 text-warning"
          aria-label={`${participant.player_name} is the monarch`}
        >
          <Crown className="size-6" />
        </span>
      )}
      <button
        type="button"
        className={cn(
          "btn btn-xs absolute top-2 right-2 h-7 min-h-0 gap-1 border-white/15 bg-black/70 px-2.5 text-xs text-white hover:bg-black/85",
          pinned && "border-primary bg-primary/80 hover:bg-primary",
        )}
        onClick={onTogglePin}
        aria-pressed={pinned}
        title={
          pinned
            ? "Pinned: this board stays active when players join"
            : "Pin this board so it stays active when players join"
        }
      >
        {pinned ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
        {pinned ? "Pinned" : "Pin"}
      </button>
    </div>
  )
}

export function CameraTile({
  participant,
  unattackable = false,
  monarch,
  stream,
  local,
  flipped = false,
  connectionState,
  hiddenLabel,
  revealBadge,
  active,
  currentTurn = false,
  onActivate,
  lifeControl,
}: {
  participant: TableParticipant
  unattackable?: boolean
  monarch?: boolean
  stream?: MediaStream
  local: boolean
  flipped?: boolean
  connectionState?: RTCPeerConnectionState
  hiddenLabel?: string
  revealBadge?: string
  active: boolean
  currentTurn?: boolean
  onActivate: () => void
  lifeControl: ReactNode
}) {
  return (
    <div
      className={cn(
        "relative block aspect-video w-full overflow-hidden rounded-sm border-2 bg-black text-left transition",
        active ? "border-primary" : "border-white/10 hover:border-white/40",
      )}
    >
      <button
        type="button"
        className="absolute inset-0 h-full w-full"
        onClick={onActivate}
        aria-pressed={active}
        aria-label={`Show ${participant.player_name}'s board`}
      >
        {hiddenLabel ? (
          <VideoPlaceholder compact label={hiddenLabel} />
        ) : stream ? (
          <StreamVideo stream={stream} className={cn("object-cover", flipped && "-scale-y-100")} />
        ) : (
          <VideoPlaceholder
            compact
            label={
              participant.departed
                ? "Left table"
                : local
                  ? "Starting camera…"
                  : describeConnection(connectionState)
            }
          />
        )}
        {participant.camera_off && <CameraOffOverlay compact />}
        {revealBadge && (
          <span className="absolute right-0 bottom-0 left-0 bg-primary px-1 py-0.5 text-center text-[0.6rem] text-primary-content">
            {revealBadge}
          </span>
        )}
      </button>
      {participant.eliminated && <EliminatedOverlay compact />}
      {unattackable && (
        <span className="pointer-events-none absolute top-7 right-1 max-w-[calc(100%-4.5rem)] rounded bg-base-100/95 px-1.5 py-1 text-[0.6rem] leading-tight font-semibold text-warning">
          Can't attack yet
        </span>
      )}
      {currentTurn && !participant.eliminated && <CurrentTurnBadge compact />}
      {lifeControl}
      {monarch && (
        <span
          className="pointer-events-none absolute top-1.5 right-1.5 rounded bg-black/80 p-1 text-warning"
          aria-label={`${participant.player_name} is the monarch`}
        >
          <Crown className="size-4" />
        </span>
      )}
    </div>
  )
}

export function OpenSeat() {
  return (
    <div className="grid aspect-video w-full place-items-center rounded-sm border-2 border-dashed border-white/10 text-white/30">
      <div className="text-center text-[0.65rem]">
        <UserPlus className="mx-auto mb-1 size-4" />
        Open seat
      </div>
    </div>
  )
}
