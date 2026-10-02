import { useEffect, useState } from "react"
import { sampledElapsed, type TimerSample } from "./game-timer"

/** Elapsed game time that advances only while the clock runs. The displays round to whole
 * seconds, so one tick per second is enough; a paused or unstarted clock does not tick and
 * does not re-render the seat table over the live video. */
export function useTimerElapsed(sample: TimerSample | null) {
  const [now, setNow] = useState(() => performance.now())
  const running = sample?.state.started_at != null && sample.state.paused_at == null
  useEffect(() => {
    if (!running) return
    const tick = window.setInterval(() => setNow(performance.now()), 1000)
    return () => window.clearInterval(tick)
  }, [running])
  return sample ? sampledElapsed(sample, Math.max(now, sample.receivedAt)) : 0
}
