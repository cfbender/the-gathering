/** Only the owner and their chosen viewer may see a private reveal. */
export function canViewBoard(owner: string, viewer: string, revealTo?: string | null) {
  return owner === viewer || revealTo === undefined || revealTo === null || revealTo === viewer
}

/** The codec every WebRTC browser ships and the one hardware encoders and decoders cover
 * (VideoToolbox, Media Foundation, VA-API); Chrome never hardware-encodes VP8. Even in
 * software, OpenH264 + FFmpeg cost about half of libvpx: measured in Chrome 154 for a
 * three-seat 1080p15 mesh, ~58% of a core per browser against ~105%. */
export const PREFERRED_VIDEO_CODEC = "video/H264"

/** Moves the preferred codec's entries to the front, keeping every other entry (including
 * rtx/red/ulpfec, which setCodecPreferences expects to stay) in its original order. */
export function orderVideoCodecs<T extends { mimeType: string }>(
  codecs: readonly T[],
  preferred = PREFERRED_VIDEO_CODEC,
): T[] {
  const wanted = preferred.toLowerCase()
  const matches = (codec: T) => codec.mimeType.toLowerCase() === wanted
  return [...codecs.filter(matches), ...codecs.filter((codec) => !matches(codec))]
}

export type PublisherQuality = "auto" | "1080p" | "720p" | "540p"

export function isPublisherQuality(value: unknown): value is PublisherQuality {
  return value === "auto" || value === "1080p" || value === "720p" || value === "540p"
}

/** Frames per second each sender encodes, by room size (including the local seat).
 *
 * The mesh runs one software encoder per remote seat and one decoder per incoming stream, and
 * both cost CPU in proportion to pixels × frames: measured in Chromium with libvpx VP8, a
 * 1080p stream at 20 fps takes about 40% of a core to encode and 30% to decode, so a four-seat
 * table at 30 fps needs roughly three cores per browser. Cards on a table do not move, so
 * tables of three or more trade frame rate for resolution, which is what reading a card needs.
 * A two-player table has one encoder and one decoder and keeps full motion. */
export function videoFrameRate(roomSize: number) {
  return roomSize <= 2 ? 30 : 15
}

/** Room size includes the local seat. Keep the native camera untouched for crop RPC.
 * Explicit quality is a ceiling: never upscale a lower-resolution camera. */
export function videoEncoding(roomSize: number, quality: PublisherQuality = "auto", height = 1080) {
  const maxFramerate = videoFrameRate(roomSize)
  if (quality !== "auto") {
    const target = { "1080p": 1080, "720p": 720, "540p": 540 }[quality]
    const bitrate = { "1080p": 2_500_000, "720p": 1_200_000, "540p": 600_000 }[quality]
    return {
      scaleResolutionDownBy: Math.max(1, height / target),
      maxBitrate: bitrate,
      maxFramerate,
    }
  }
  if (roomSize <= 4) return { scaleResolutionDownBy: 1, maxBitrate: 2_500_000, maxFramerate }
  if (roomSize <= 7) return { scaleResolutionDownBy: 1.5, maxBitrate: 1_200_000, maxFramerate }
  return { scaleResolutionDownBy: 2, maxBitrate: 600_000, maxFramerate }
}
