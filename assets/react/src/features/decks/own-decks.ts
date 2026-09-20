import type { DeckSummary } from "@/features/decks/decks"

/** Decks owned by the viewer's linked player. */
export function ownDecks(decks: DeckSummary[], viewerId: number | undefined) {
  return decks.filter((deck) => viewerId !== undefined && deck.player?.user_id === viewerId)
}
