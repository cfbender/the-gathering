import { useEffect, useState } from "react"
import { sampledElapsed, type TimerSample } from "./game-timer"

export function useTimerElapsed(sample: TimerSample | null) {
  const [now, setNow] = useState(() => performance.now())
  useEffect(() => {
    const tick = window.setInterval(() => setNow(performance.now()), 250)
    return () => window.clearInterval(tick)
  }, [])
  return sample ? sampledElapsed(sample, Math.max(now, sample.receivedAt)) : 0
}
