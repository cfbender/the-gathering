import { Pause, Play, Timer } from "lucide-react"
import { useEffect, useState } from "react"
import { formatElapsed, sampledElapsed, type TimerSample } from "./game-timer"

export function TableTimer({
  sample,
  onChange,
}: {
  sample: TimerSample | null
  onChange: (action: "pause" | "resume") => void
}) {
  const [now, setNow] = useState(() => performance.now())
  useEffect(() => {
    const tick = window.setInterval(() => setNow(performance.now()), 250)
    return () => window.clearInterval(tick)
  }, [])
  const started = sample?.state.started_at != null
  const paused = sample?.state.paused_at != null

  return (
    <div
      className="flex items-center justify-between gap-3 border-t border-white/10 bg-zinc-950 px-4 py-2"
      aria-label="Game timer"
    >
      <div className="flex items-center gap-3">
        <Timer className="size-4 text-violet-300" />
        <span className="text-xl font-semibold tracking-wider text-white tabular-nums" role="timer">
          {formatElapsed(sample ? sampledElapsed(sample, Math.max(now, sample.receivedAt)) : 0)}
        </span>
        <span className="text-xs text-white/50">
          {!started ? "Ready to start" : paused ? "Paused" : "Playing"}
        </span>
      </div>
      {started && (
        <button
          type="button"
          className="btn btn-ghost btn-sm gap-1.5 text-xs"
          disabled={!sample}
          onClick={() => onChange(paused ? "resume" : "pause")}
        >
          {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
          {paused ? "Resume timer" : "Pause timer"}
        </button>
      )}
    </div>
  )
}
