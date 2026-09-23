import type { GameTimerState } from "./game-timer"
import type { BoardCard, IdentifiedCard } from "./use-webcam-room"

/** Everything identified on one board goes away; the other boards keep their cards. */
export function clearBoardCards(entries: BoardCard[], ownerPeerId: string): BoardCard[] {
  return entries.filter((entry) => entry.ownerPeerId !== ownerPeerId)
}

/** A game starts when the shared timer first gains a start time. A late joiner's initial
 * timer sample already has one, and must not wipe the cards it is about to be synced. */
export function gameJustStarted(
  previous: GameTimerState | null | undefined,
  next: GameTimerState,
): boolean {
  return previous != null && previous.started_at == null && next.started_at != null
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
