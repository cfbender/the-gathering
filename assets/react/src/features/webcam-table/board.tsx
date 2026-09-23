import { Pin, PinOff, UserPlus, Video, VideoOff } from "lucide-react"
import type { MouseEvent } from "react"
import { cn } from "@/lib/cn"
import type { TableParticipant } from "./use-webcam-room"

export function StreamVideo({
  stream,
  muted = false,
  className,
}: {
  stream: MediaStream
  muted?: boolean
  className?: string
}) {
  return (
    <video
      ref={(video) => {
        if (video && video.srcObject !== stream) video.srcObject = stream
      }}
      className={cn("h-full w-full", className)}
      autoPlay
      playsInline
      muted={muted}
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

/** Life total badge overlaid on the top-left of a board, like a table scoreboard. */
export function LifeBadge({ life, size }: { life: number; size: "board" | "tile" }) {
  return (
    <div
      className={cn(
        "pointer-events-none absolute top-1.5 left-1.5 grid place-items-center rounded bg-black/80 font-black text-white tabular-nums shadow",
        size === "board" ? "min-w-14 px-2 py-1 text-3xl md:text-4xl" : "min-w-7 px-1.5 text-sm",
      )}
      aria-label={`${life} life`}
    >
      {life}
    </div>
  )
}

/** Maps a click on a `object-contain` video to normalized source coordinates. */
export function capturePoint(event: MouseEvent<HTMLElement>) {
  const video = event.currentTarget.querySelector("video")
  if (!video || !video.videoWidth || !video.videoHeight) return null

  const bounds = event.currentTarget.getBoundingClientRect()
  const sourceRatio = video.videoWidth / video.videoHeight
  const boundsRatio = bounds.width / bounds.height
  const renderedWidth = sourceRatio > boundsRatio ? bounds.width : bounds.height * sourceRatio
  const renderedHeight = sourceRatio > boundsRatio ? bounds.width / sourceRatio : bounds.height
  const left = bounds.left + (bounds.width - renderedWidth) / 2
  const top = bounds.top + (bounds.height - renderedHeight) / 2

  return {
    x: Math.max(0, Math.min(1, (event.clientX - left) / renderedWidth)),
    y: Math.max(0, Math.min(1, (event.clientY - top) / renderedHeight)),
  }
}

export function ActiveBoard({
  participant,
  stream,
  local,
  pinned,
  onTogglePin,
  onInspect,
}: {
  participant: TableParticipant
  stream?: MediaStream
  local: boolean
  pinned: boolean
  onTogglePin: () => void
  onInspect: (event: MouseEvent<HTMLButtonElement>) => void
}) {
  return (
    <div className="relative h-full w-full overflow-hidden bg-black">
      <button
        type="button"
        className="group relative block h-full w-full cursor-crosshair text-left"
        onClick={onInspect}
        aria-label={`Inspect ${participant.player_name}'s board`}
      >
        {stream ? (
          <StreamVideo stream={stream} muted={local} className="object-contain" />
        ) : (
          <VideoPlaceholder label={local ? "Starting camera…" : "Connecting…"} />
        )}
        {participant.camera_off && <CameraOffOverlay />}
        <span className="pointer-events-none absolute bottom-9 left-1/2 -translate-x-1/2 rounded-full bg-black/75 px-3 py-1 text-xs text-white opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
          Click a card to identify it · Shift+click to choose
        </span>
      </button>
      <LifeBadge life={participant.life} size="board" />
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
  stream,
  local,
  active,
  onActivate,
}: {
  participant: TableParticipant
  stream?: MediaStream
  local: boolean
  active: boolean
  onActivate: () => void
}) {
  return (
    <button
      type="button"
      className={cn(
        "relative block aspect-video w-full overflow-hidden rounded-sm border-2 bg-black text-left transition",
        active ? "border-primary" : "border-white/10 hover:border-white/40",
      )}
      onClick={onActivate}
      aria-pressed={active}
      aria-label={`Show ${participant.player_name}'s board`}
    >
      {stream ? (
        <StreamVideo stream={stream} muted={local} className="object-cover" />
      ) : (
        <VideoPlaceholder compact label={local ? "Starting camera…" : "Connecting…"} />
      )}
      {participant.camera_off && <CameraOffOverlay compact />}
      <LifeBadge life={participant.life} size="tile" />
    </button>
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
