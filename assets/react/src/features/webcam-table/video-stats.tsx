import { useEffect, useState } from "react"

interface Sample {
  id: string
  bytes: number
  timestamp: number
}
export interface VideoStats {
  width?: number
  height?: number
  fps?: number
  kbps?: number
  candidate?: string
  sample?: Sample
}

/** RTP byte deltas, not lifetime averages; a replaced report resets the baseline. */
export function summarizeVideoStats(report: RTCStatsReport, previous?: Sample): VideoStats {
  let video: RTCInboundRtpStreamStats | undefined
  let pairId: string | undefined
  report.forEach((stat) => {
    if (stat.type === "inbound-rtp" && stat.kind === "video") video = stat
    if (stat.type === "transport") pairId = stat.selectedCandidatePairId
  })
  const pair = pairId ? report.get(pairId) : undefined
  const candidate = pair?.remoteCandidateId
    ? report.get(pair.remoteCandidateId)?.candidateType
    : undefined
  if (!video) return { candidate }
  const sample = { id: video.id, bytes: video.bytesReceived ?? 0, timestamp: video.timestamp }
  const delta =
    previous &&
    sample.id === previous.id &&
    sample.timestamp > previous.timestamp &&
    sample.bytes >= previous.bytes
      ? ((sample.bytes - previous.bytes) * 8) / (sample.timestamp - previous.timestamp)
      : undefined
  return {
    width: video.frameWidth,
    height: video.frameHeight,
    fps: video.framesPerSecond,
    kbps: delta,
    candidate,
    sample,
  }
}

export function useVideoStats(
  enabled: boolean,
  getStats: () => Promise<{ id: string; report: RTCStatsReport }[]>,
) {
  const [stats, setStats] = useState<Record<string, VideoStats>>({})
  useEffect(() => {
    if (!enabled) return
    let disposed = false
    let timer: number
    let previous: Record<string, VideoStats> = {}
    const poll = async () => {
      try {
        const reports = await getStats()
        if (disposed) return
        const next = Object.fromEntries(
          reports.map(({ id, report }) => [id, summarizeVideoStats(report, previous[id]?.sample)]),
        )
        previous = next
        setStats(next)
      } catch {
        // A connection can close during sampling. The next poll refreshes the live set.
      } finally {
        if (!disposed) timer = window.setTimeout(poll, 2000)
      }
    }
    void poll()
    return () => {
      disposed = true
      window.clearTimeout(timer)
    }
  }, [enabled, getStats])
  return enabled ? stats : {}
}

export function VideoStatsOverlay({
  stats,
  localStream,
}: {
  stats?: VideoStats
  localStream?: MediaStream | null
}) {
  const settings = localStream?.getVideoTracks()[0]?.getSettings()
  return (
    <span
      className="pointer-events-none absolute top-1 right-8 max-w-[calc(100%-5rem)] rounded bg-black/85 px-1.5 py-1 text-[0.6rem] text-base-content tabular-nums"
      aria-label="Video statistics"
    >
      {settings?.width ?? stats?.width ?? "?"} × {settings?.height ?? stats?.height ?? "?"} ·{" "}
      {Math.round(settings?.frameRate ?? stats?.fps ?? 0)} fps
      <br />
      {localStream
        ? "Local capture"
        : `${stats?.kbps === undefined ? "…" : Math.round(stats.kbps)} kbps · ${stats?.candidate ?? "connecting"}`}
    </span>
  )
}
