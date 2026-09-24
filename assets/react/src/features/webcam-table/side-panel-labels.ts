import type { RecognizerState } from "./recognition/use-recognizer"

export function describeIceServers(servers: RTCIceServer[]): string {
  const urls = servers.flatMap((server) =>
    Array.isArray(server.urls) ? server.urls : [server.urls],
  )
  const stun = urls.filter((url) => url.startsWith("stun:")).length
  const turn = urls.filter((url) => url.startsWith("turn:") || url.startsWith("turns:")).length
  if (stun === 0 && turn === 0) return "no STUN or TURN — same network only"
  const parts = []
  if (stun > 0) parts.push(`${stun} STUN`)
  if (turn > 0) parts.push(`${turn} TURN`)
  return parts.join(", ") + (turn === 0 ? " (no relay)" : "")
}

export function describeRecognizer(state: RecognizerState): string {
  switch (state.status) {
    case "idle":
      return "Card scanning loads in the background once you are connected to the table."
    case "checking":
      return "Checking for a recognition bundle…"
    case "unavailable":
      return "No recognition bundle is installed on this server; clicks offer the seat's decks instead. Publish one with `python -m cardid.publish`."
    case "loading":
      return `Loading bundle ${state.version}…`
    case "ready":
      return `Bundle ${state.version}: ${state.arts.toLocaleString()} artworks, loaded in ${(state.loadMs / 1000).toFixed(1)} s.`
    case "failed":
      return `Recognizer failed to start: ${state.message}`
  }
}
