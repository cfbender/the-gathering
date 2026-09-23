/** Only the owner and their chosen viewer may see a private reveal. */
export function canViewBoard(owner: string, viewer: string, revealTo?: string | null) {
  return owner === viewer || revealTo === undefined || revealTo === null || revealTo === viewer
}

export type PublisherQuality = "auto" | "1080p" | "720p" | "540p"

export function isPublisherQuality(value: unknown): value is PublisherQuality {
  return value === "auto" || value === "1080p" || value === "720p" || value === "540p"
}

/** Room size includes the local seat. Keep the native camera untouched for crop RPC.
 * Explicit quality is a ceiling: never upscale a lower-resolution camera. */
export function videoEncoding(roomSize: number, quality: PublisherQuality = "auto", height = 1080) {
  if (quality !== "auto") {
    const target = { "1080p": 1080, "720p": 720, "540p": 540 }[quality]
    const bitrate = { "1080p": 2_500_000, "720p": 1_200_000, "540p": 600_000 }[quality]
    return { scaleResolutionDownBy: Math.max(1, height / target), maxBitrate: bitrate }
  }
  if (roomSize <= 4) return { scaleResolutionDownBy: 1, maxBitrate: 2_500_000 }
  if (roomSize <= 7) return { scaleResolutionDownBy: 1.5, maxBitrate: 1_200_000 }
  return { scaleResolutionDownBy: 2, maxBitrate: 600_000 }
}
