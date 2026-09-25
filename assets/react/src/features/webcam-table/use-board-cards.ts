import { useQueryClient } from "@tanstack/react-query"
import type { Channel } from "phoenix"
import { useCallback, useRef, useState } from "react"
import { prefetchPrintings } from "./card-details"
import { applyCardCommand, sameCard, type CardCommand } from "./identified-cards"
import type { RoomLink } from "./room-link"
import type { BoardCard, IdentifiedCard } from "./room-types"

/** The server stamps who identified a card from the sender's seat, so the local overlay keeps
 * `byPlayerName` for display but the push leaves it out. */
function wirePayload(command: CardCommand) {
  if (command.type !== "card_identified") return command
  const { byPlayerName: _stampedByServer, ...entry } = command.entry
  return { ...command, entry }
}

/** The table's identified cards. The server owns the list and broadcasts all of it on every
 * change (and starts every game empty); this seat's own changes show immediately and stay
 * overlaid only until the server answers the push that carries them. */
export function useBoardCards(link: RoomLink) {
  const cardsRef = useRef<BoardCard[]>([])
  const serverCardsRef = useRef<BoardCard[]>([])
  const pendingRef = useRef(new Map<number, CardCommand>())
  const commandIdRef = useRef(0)
  const [identifiedCards, setIdentifiedCards] = useState<BoardCard[]>([])
  const queryClient = useQueryClient()

  const show = useCallback(() => {
    cardsRef.current = [...pendingRef.current.values()].reduce(
      applyCardCommand,
      serverCardsRef.current,
    )
    setIdentifiedCards(cardsRef.current)
  }, [])

  /** Cards new to this seat, including everything already on the table when it joins mid-game,
   * are fetched in the background so opening them is instant. Newest first: those are the
   * ones someone is about to open. */
  const receive = useCallback(
    (entries: BoardCard[]) => {
      const known = new Set(serverCardsRef.current.map((entry) => entry.card.id))
      serverCardsRef.current = entries
      show()
      const added = entries
        .map((entry) => entry.card.id)
        .filter((id) => !known.has(id))
        .reverse()
      if (added.length > 0) void prefetchPrintings(queryClient, new Set(added))
    },
    [queryClient, show],
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
        .push("cards", wirePayload(command))
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
