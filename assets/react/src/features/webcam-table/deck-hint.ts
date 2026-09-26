import type { DecklistCards } from "@/features/decks/decklist-cards"
import type { Candidate, GalleryArt } from "./recognition/pipeline"

/**
 * Cosine bonus for a candidate that is in the clicked board owner's linked deck list. It can
 * reorder the recognizer's top five and settle a near-tie, but a non-deck card that leads a
 * deck card by more than this still wins (stolen cards, copies, an outdated list).
 * Checked against the gallery-wide margin (`CLEAR_MARGIN`) in `ml/` evaluation; see
 * `ml/README.md` "Deck-list prior".
 */
export const DECK_PRIOR = 0.03

/** What the scanner knows about one seat's deck: names to favour and, per gallery art, the
 * exact printing the list names (so a recognized art records the player's own printing). */
export interface DeckHint {
  names: ReadonlySet<string>
  printings: ReadonlyMap<string, GalleryArt>
}

export type HintedCandidate = Candidate & { inDeck?: boolean }

/** Case- and accent-insensitive, like the catalog's `normalized_name`. */
export function normalizeCardName(name: string) {
  return name
    .normalize("NFD")
    .replace(/\p{Mn}/gu, "")
    .toLowerCase()
    .trim()
}

/** Whole names and each face of `A // B` cards, since gallery arts of separately printed
 * sides carry the face name. */
export function deckNames(list: { cards: Array<{ name: string }> }): Set<string> {
  const names = new Set<string>()
  for (const card of list.cards) {
    const name = normalizeCardName(card.name)
    names.add(name)
    for (const face of name.split(" // ")) names.add(face)
  }
  return names
}

export function inDeck(name: string, names: ReadonlySet<string> | undefined) {
  if (!names || names.size === 0) return false
  const normalized = normalizeCardName(name)
  return names.has(normalized) || normalized.split(" // ").some((face) => names.has(face))
}

/** Adds `prior` to candidates in the deck and re-ranks; everything else is untouched, so a
 * seat without a list sees exactly the recognizer's answer. */
export function applyDeckHint(
  candidates: Candidate[],
  names: ReadonlySet<string> | undefined,
  prior = DECK_PRIOR,
): HintedCandidate[] {
  if (!names || names.size === 0) return candidates
  return candidates
    .map((candidate) =>
      inDeck(candidate.name, names)
        ? { ...candidate, inDeck: true, score: candidate.score + prior }
        : candidate,
    )
    .sort((a, b) => b.score - a.score)
}

/** Gallery search results with the deck's cards first, otherwise in the search's order. */
export function deckFirst<T extends { name: string }>(
  arts: T[],
  names: ReadonlySet<string> | undefined,
): T[] {
  if (!names || names.size === 0) return arts
  return [
    ...arts.filter((art) => inDeck(art.name, names)),
    ...arts.filter((art) => !inDeck(art.name, names)),
  ]
}

/** The printing IDs a list names, for the worker's `locate`. */
export function listPrintingIds(list: Pick<DecklistCards, "cards">): string[] {
  return list.cards.flatMap((card) => (card.printing_id ? [card.printing_id] : []))
}

/** One seat's hint. `located` is the worker's answer to `locate` (arts whose `printings` are
 * narrowed to requested IDs, possibly for several seats); only this list's printings count. */
export function deckHint(list: Pick<DecklistCards, "cards">, located: GalleryArt[] = []): DeckHint {
  const ids = new Set(listPrintingIds(list))
  const printings = new Map<string, GalleryArt>()
  for (const art of located) {
    const printing = art.printings?.find((candidate) => ids.has(candidate.id))
    if (printing && !printings.has(art.id)) printings.set(art.id, { ...printing, frame: art.frame })
  }
  return { names: deckNames(list), printings }
}
