import type { DeckSummary } from "@/features/decks/decks"

/** Decks owned by the viewer's linked player. */
export function ownDecks(decks: DeckSummary[], viewerId: number | undefined) {
  return decks.filter((deck) => viewerId !== undefined && deck.player?.user_id === viewerId)
}

export type DeckScope = "mine" | "all"

/** Reads the `?scope=` search param; anything unrecognised means "not chosen". */
export function parseDeckScope(value: unknown): DeckScope | undefined {
  return value === "mine" || value === "all" ? value : undefined
}

/**
 * Scope to show: the explicit choice when present, otherwise the viewer's own
 * decks, falling back to everyone's when they have none (no linked player yet,
 * or nothing logged) so the page is never empty by default.
 */
export function resolveDeckScope(choice: DeckScope | undefined, ownCount: number): DeckScope {
  return choice ?? (ownCount > 0 ? "mine" : "all")
}
