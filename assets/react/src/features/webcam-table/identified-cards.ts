import type { BoardCard, IdentifiedCard } from "./use-webcam-room"

/** The recognition gallery has full names, not oracle IDs. Keep both faces of a card's name. */
export function sameCard(a: IdentifiedCard, b: IdentifiedCard) {
  return a.name.trim().toLowerCase() === b.name.trim().toLowerCase()
}

/** Oldest entry wins, with an ID tie-break so simultaneous discoveries converge at every seat. */
export function mergeIdentifiedCards(current: BoardCard[], incoming: BoardCard[]): BoardCard[] {
  const entries = [
    ...new Map([...current, ...incoming].map((entry) => [entry.id, entry])).values(),
  ].sort((a, b) => a.at - b.at || a.id.localeCompare(b.id))
  return entries.filter(
    (entry, index) =>
      !entries
        .slice(0, index)
        .some(
          (previous) =>
            previous.ownerPeerId === entry.ownerPeerId && sameCard(previous.card, entry.card),
        ),
  )
}
