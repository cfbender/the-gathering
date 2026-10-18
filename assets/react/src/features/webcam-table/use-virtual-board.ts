import {
  createPlaytestState,
  initialSnapshot,
  nextTurnSnapshot,
  projectPublicSnapshot,
  type PlaytestSnapshot,
  type PlaytestState,
} from "@cfbender/playtest"
import { useQuery } from "@tanstack/react-query"
import { useCallback, useEffect, useRef, useState } from "react"
import { decklistCardsQuery, hasDecklistCards } from "@/features/decks/decklist-cards"
import type { DeckSummary } from "@/features/decks/decks"
import { turnId } from "./game-modes"
import { dealDecklist } from "./virtual-board"
import type { useWebcamRoom } from "./use-webcam-room"

interface SavedBoard {
  version: 1
  deckName: string
  /** The deal, kept so a rematch can shuffle the same cards into a new game. */
  initialState: PlaytestState
  snapshot: PlaytestSnapshot
}

function storageKey(roomId: string, playerId: number) {
  return `the-gathering:virtual-board:${roomId}:${playerId}`
}

function load(key: string): SavedBoard | null {
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null") as SavedBoard | null
    return saved?.version === 1 ? saved : null
  } catch {
    return null
  }
}

/** A fresh game from the same cards. */
function redeal(state: PlaytestState) {
  return createPlaytestState([...state.library, ...state.hand], state.command)
}

/**
 * This seat's virtual board: the whole game stays in this browser (saved per table, so a reload
 * resumes it), and only its public projection goes to the table. The board is republished
 * whenever the table lacks it (a rejoin or a rematch), redealt when a rematch resets the seat,
 * and untapped with a draw when the turn passes to this seat.
 */
export function useVirtualBoard({
  room,
  roomId,
  playerId,
  deck,
  setCameraOff,
}: {
  room: ReturnType<typeof useWebcamRoom>
  roomId: string
  playerId: number
  deck: DeckSummary | undefined
  setCameraOff: (off: boolean) => void
}) {
  const key = storageKey(roomId, playerId)
  const [saved, setSaved] = useState<SavedBoard | null>(() => load(key))
  const savedRef = useRef(saved)
  const listed = !!deck && hasDecklistCards(deck)
  const list = useQuery({ ...decklistCardsQuery(deck?.id ?? 0), enabled: listed })
  const { publishBoard, boards, seatResets, turns, participants, mode } = room

  const store = useCallback(
    (next: SavedBoard | null) => {
      savedRef.current = next
      setSaved(next)
      if (next) localStorage.setItem(key, JSON.stringify(next))
      else localStorage.removeItem(key)
      publishBoard(next ? projectPublicSnapshot(next.snapshot) : null)
    },
    [key, publishBoard],
  )

  const change = useCallback(
    (snapshot: PlaytestSnapshot) => {
      const current = savedRef.current
      if (current) store({ ...current, snapshot })
    },
    [store],
  )

  // The table lost this board (a rejoin after a restart, or a rematch): publish it again.
  const published = playerId in boards
  useEffect(() => {
    if (savedRef.current && !published)
      publishBoard(projectPublicSnapshot(savedRef.current.snapshot))
  }, [boards, published, publishBoard])

  const firstReset = useRef(seatResets)
  useEffect(() => {
    const current = savedRef.current
    if (seatResets === firstReset.current || !current) return
    firstReset.current = seatResets
    const initialState = redeal(current.initialState)
    store({ ...current, initialState, snapshot: initialSnapshot(initialState) })
  }, [seatResets, store])

  const myTurn = turnId(participants, playerId, mode) ?? playerId
  const previousTurn = useRef(turns.active_player_id)
  useEffect(() => {
    const previous = previousTurn.current
    previousTurn.current = turns.active_player_id
    const current = savedRef.current
    if (current && previous !== null && previous !== myTurn && turns.active_player_id === myTurn)
      change(nextTurnSnapshot(current.snapshot, { draw: true }))
  }, [change, myTurn, turns.active_player_id])

  return {
    /** The seat plays on its virtual board instead of the camera. */
    active: saved !== null,
    saved,
    /** Why the board cannot start, or null when it can. */
    unavailable: room.spectating
      ? "Spectators have no board"
      : !deck
        ? "Choose a commander first"
        : !listed
          ? "Link the deck's list to use a virtual board"
          : list.isError
            ? "Couldn't load the deck's list"
            : !list.data
              ? "Loading the deck's list…"
              : null,
    start: () => {
      if (!list.data || !deck) return
      const initialState = dealDecklist(list.data)
      store({
        version: 1,
        deckName: deck.name,
        initialState,
        snapshot: initialSnapshot(initialState),
      })
      setCameraOff(true)
    },
    stop: () => {
      store(null)
      setCameraOff(false)
    },
    change,
  }
}

export type VirtualBoard = ReturnType<typeof useVirtualBoard>
