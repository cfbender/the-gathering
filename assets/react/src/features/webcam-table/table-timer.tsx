import { Pause, Play, Timer } from "lucide-react"
import { cn } from "@/lib/cn"
import { formatElapsed, type TimerSample } from "./game-timer"
import { useTimerElapsed } from "./use-timer-elapsed"

/** Compact elapsed-time badge for the Table tab header. Non-interactive because the section
 * header is itself a button. */
export function TimerBadge({ sample }: { sample: TimerSample | null }) {
  const elapsed = useTimerElapsed(sample)
  const started = sample?.state.started_at != null
  const paused = sample?.state.paused_at != null
  if (!started) return null
  return (
    <span
      role="timer"
      aria-label={`Game timer ${paused ? "paused" : "running"}`}
      className={cn(
        "badge badge-sm gap-1 border-white/10 font-semibold tabular-nums",
        paused ? "bg-base-300 text-base-content/60" : "bg-primary/20 text-base-content",
      )}
    >
      {paused ? <Pause className="size-3" /> : <Timer className="size-3" />}
      {formatElapsed(elapsed)}
    </span>
  )
}

/** Pause/resume action for the Table tab; hidden until the match has started. */
export function TimerToggle({
  sample,
  onChange,
}: {
  sample: TimerSample | null
  onChange: (action: "pause" | "resume") => void
}) {
  const started = sample?.state.started_at != null
  const paused = sample?.state.paused_at != null
  if (!started) return null
  return (
    <button
      type="button"
      className="btn btn-ghost btn-sm w-full text-xs"
      onClick={() => onChange(paused ? "resume" : "pause")}
    >
      {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
      {paused ? "Resume timer" : "Pause timer"}
    </button>
  )
}
