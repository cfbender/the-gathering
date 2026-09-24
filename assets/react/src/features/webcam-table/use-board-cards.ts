import type { Channel } from "phoenix"
import { useCallback, useRef, useState } from "react"
import { applyCardCommand, sameCard, type CardCommand } from "./identified-cards"
import type { RoomLink } from "./room-link"
import type { BoardCard, IdentifiedCard } from "./room-types"

/** The table's identified cards. The server owns the list and broadcasts all of it on every
 * change (and starts every game empty); this seat's own changes show immediately and stay
 * overlaid only until the server answers the push that carries them. */
export function useBoardCards(link: RoomLink) {
  const cardsRef = useRef<BoardCard[]>([])
  const serverCardsRef = useRef<BoardCard[]>([])
  const pendingRef = useRef(new Map<number, CardCommand>())
  const commandIdRef = useRef(0)
  const [identifiedCards, setIdentifiedCards] = useState<BoardCard[]>([])

  const show = useCallback(() => {
    cardsRef.current = [...pendingRef.current.values()].reduce(
      applyCardCommand,
      serverCardsRef.current,
    )
    setIdentifiedCards(cardsRef.current)
  }, [])

  const receive = useCallback(
    (entries: BoardCard[]) => {
      serverCardsRef.current = entries
      show()
    },
    [show],
  )

  /** The server broadcasts its list before replying, so an accepted change never flickers; a
   * rejected or timed-out change falls back to the server's list. */
  const send = useCallback(
    (command: CardCommand) => {
      const channel = link.channel
      if (link.spectator || !channel) return
      const id = (commandIdRef.current += 1)
      pendingRef.current.set(id, command)
      show()
      const settle = () => {
        pendingRef.current.delete(id)
        show()
      }
      channel
        .push("cards", command)
        .receive("ok", settle)
        .receive("error", settle)
        .receive("timeout", settle)
    },
    [link, show],
  )

  const bindChannel = useCallback(
    (room: Channel) => {
      room.on("identified_cards", ({ entries }: { entries: BoardCard[] }) => receive(entries))
      room.on("table_state", ({ cards }: { cards: BoardCard[] }) => receive(cards))
    },
    [receive],
  )

  /** Names a card on `ownerPeerId`'s board and returns its entry, reusing the board's existing
   * entry for the same card. Private boards get an entry for the preview but are never
   * published: identifying a hand must not put its card names in the shared tray. */
  const announceCard = useCallback(
    (ownerPeerId: string, byPlayerName: string, card: IdentifiedCard, publish: boolean) => {
      const existing = cardsRef.current.find(
        (entry) => entry.ownerPeerId === ownerPeerId && sameCard(entry.card, card),
      )
      if (existing) return existing
      const entry: BoardCard = {
        id: crypto.randomUUID(),
        ownerPeerId,
        byPlayerName,
        card,
        at: Date.now(),
      }
      if (publish) send({ type: "card_identified", entry })
      return entry
    },
    [send],
  )

  /** Takes a misidentified card off its board's list at every seat. */
  const removeCard = useCallback((id: string) => send({ type: "card_removed", id }), [send])

  /** Empties the local seat's own board at every seat; other boards are not ours to clear. */
  const clearOwnCards = useCallback(
    () => send({ type: "cards_cleared", ownerPeerId: link.peerId }),
    [link, send],
  )

  return { identifiedCards, bindChannel, announceCard, removeCard, clearOwnCards }
}
