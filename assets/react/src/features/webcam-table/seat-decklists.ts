import { useQueries, type UseQueryResult } from "@tanstack/react-query"
import { useEffect, useMemo } from "react"
import {
  decklistCardsQuery,
  hasDecklistCards,
  type DecklistCards,
} from "@/features/decks/decklist-cards"
import type { DeckSummary } from "@/features/decks/decks"
import type { TableParticipant } from "./room-types"

/** Loaded deck lists by seat (`peer_id`); seats without a readable list are absent. */
export type SeatDecklists = ReadonlyMap<string, DecklistCards>

const listData = (results: UseQueryResult<DecklistCards>[]) => results.map((result) => result.data)

/**
 * Fetches the linked list of every seated player's chosen deck and warms the browser cache
 * with its images, so the list opens instantly and the scanner can favour those cards.
 * Every seat loads every list; anyone watching the network tab can read opponents' lists.
 */
export function useSeatDecklists(seated: TableParticipant[], decks: DeckSummary[]): SeatDecklists {
  const seatDecks = seated.flatMap((seat) => {
    const deck = decks.find((candidate) => candidate.id === seat.deck_id)
    return deck && hasDecklistCards(deck) ? [{ peerId: seat.peer_id, deckId: deck.id }] : []
  })
  // `listData` is stable, so TanStack returns the same array until a list changes.
  const lists = useQueries({
    queries: seatDecks.map((seat) => decklistCardsQuery(seat.deckId)),
    combine: listData,
  })
  const peers = JSON.stringify(seatDecks.map((seat) => seat.peerId))

  useEffect(() => {
    prefetchDecklistImages(lists.filter((list) => list !== undefined))
  }, [lists])

  return useMemo(() => {
    const byPeer = new Map<string, DecklistCards>()
    ;(JSON.parse(peers) as string[]).forEach((peer, index) => {
      const list = lists[index]
      if (list) byPeer.set(peer, list)
    })
    return byPeer
  }, [lists, peers])
}

/** Thumbnails for every list first, then the full-size previews. */
export function prefetchDecklistImages(lists: DecklistCards[]) {
  const cards = lists.flatMap((list) => list.cards)
  imageQueue.add([
    ...cards.map((card) => card.image_uris.small),
    ...cards.map((card) => card.image_uris.normal),
  ])
}

/**
 * Loads images a few at a time, each URL once per page. Four seats of 100 cards would
 * otherwise start 800 requests at once and overflow the server's image cache queue
 * (`CardImages` answers 502 beyond 128 pending cold fetches).
 */
export function createImageQueue(load: (src: string) => Promise<unknown>, concurrency = 4) {
  const seen = new Set<string>()
  const waiting: string[] = []
  let active = 0

  function pump() {
    while (active < concurrency && waiting.length > 0) {
      const src = waiting.shift() as string
      active += 1
      void load(src)
        .catch(() => undefined)
        .finally(() => {
          active -= 1
          pump()
        })
    }
  }

  return {
    add(urls: Array<string | undefined>) {
      for (const url of urls) {
        if (!url || seen.has(url)) continue
        seen.add(url)
        waiting.push(url)
      }
      pump()
    },
  }
}

function loadImage(src: string) {
  return new Promise<void>((resolve, reject) => {
    const image = new Image()
    image.fetchPriority = "low"
    image.decoding = "async"
    image.onload = () => resolve()
    image.onerror = () => reject(new Error(`failed to load ${src}`))
    image.src = src
  })
}

const imageQueue = createImageQueue(loadImage)
