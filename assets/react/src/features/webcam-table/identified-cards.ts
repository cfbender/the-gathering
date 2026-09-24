import type { BoardCard, IdentifiedCard } from "./use-webcam-room"

/** A change to the shared card list that this seat asks the server to make. The server
 * validates it, applies it, and broadcasts the authoritative list as `identified_cards`. */
export type CardCommand =
  | { type: "card_identified"; entry: BoardCard }
  | { type: "card_removed"; id: string }
  /** The board's owner clears everything identified on it. */
  | { type: "cards_cleared"; ownerPeerId: string }

/** Everything identified on one board goes away; the other boards keep their cards. */
export function clearBoardCards(entries: BoardCard[], ownerPeerId: string): BoardCard[] {
  return entries.filter((entry) => entry.ownerPeerId !== ownerPeerId)
}

/** Dedupe by the displayed name: separate printed sides stay distinct, shared-art names stay whole. */
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

/** Shows a command the server has not answered yet on top of the last list it broadcast. */
export function applyCardCommand(entries: BoardCard[], command: CardCommand): BoardCard[] {
  switch (command.type) {
    case "card_identified":
      return mergeIdentifiedCards(entries, [command.entry])
    case "card_removed":
      return entries.filter((entry) => entry.id !== command.id)
    case "cards_cleared":
      return clearBoardCards(entries, command.ownerPeerId)
  }
}
