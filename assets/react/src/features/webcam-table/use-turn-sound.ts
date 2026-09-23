import { useEffect, useRef } from "react"

/** Prime the context on a user gesture; never chime merely because a room was opened. */
export function useTurnSound(enabled: boolean, activePlayerId: number | null, playerId: number) {
  const audio = useRef<AudioContext | null>(null)
  const previous = useRef(activePlayerId)
  useEffect(() => {
    if (!enabled) return
    const prime = () => {
      audio.current ??= new AudioContext()
      void audio.current.resume().catch(() => {})
    }
    window.addEventListener("pointerdown", prime)
    window.addEventListener("keydown", prime)
    return () => {
      window.removeEventListener("pointerdown", prime)
      window.removeEventListener("keydown", prime)
      void audio.current?.close()
      audio.current = null
    }
  }, [enabled])
  useEffect(() => {
    const changed = previous.current !== activePlayerId
    previous.current = activePlayerId
    const context = audio.current
    if (!enabled || !changed || activePlayerId !== playerId || context?.state !== "running") return
    const oscillator = context.createOscillator()
    const gain = context.createGain()
    oscillator.frequency.setValueAtTime(660, context.currentTime)
    oscillator.frequency.setValueAtTime(880, context.currentTime + 0.12)
    gain.gain.setValueAtTime(0.08, context.currentTime)
    gain.gain.exponentialRampToValueAtTime(0.001, context.currentTime + 0.3)
    oscillator.connect(gain).connect(context.destination)
    oscillator.onended = () => {
      oscillator.disconnect()
      gain.disconnect()
    }
    oscillator.start()
    oscillator.stop(context.currentTime + 0.3)
  }, [activePlayerId, enabled, playerId])
}
