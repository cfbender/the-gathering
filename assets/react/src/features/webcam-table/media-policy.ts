/** Only the owner and their chosen viewer may see a private reveal. */
export function canViewBoard(owner: string, viewer: string, revealTo?: string | null) {
  return owner === viewer || revealTo === undefined || revealTo === null || revealTo === viewer
}

/** Room size includes the local seat. Keep the native camera untouched for crop RPC. */
export function videoEncoding(roomSize: number) {
  if (roomSize <= 4) return { scaleResolutionDownBy: 1, maxBitrate: 2_500_000 }
  if (roomSize <= 7) return { scaleResolutionDownBy: 1.5, maxBitrate: 1_200_000 }
  return { scaleResolutionDownBy: 2, maxBitrate: 600_000 }
}
