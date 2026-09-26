import { useQueryClient } from "@tanstack/react-query"
import type { Channel } from "phoenix"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { GameFormat } from "@/features/games/game-format"
import type { GameTimerState, TimerSample } from "./game-timer"
import type { RoomLink } from "./room-link"
import type { BoardCard, Monarch, SeatStatus, TableParticipant } from "./room-types"
import { EMPTY_COUNTERS, changeCounter, type Counter } from "./seat-counters"
import {
  orderBySeats,
  retainEliminatedSeats,
  receiveTableEvent,
  toTableEvent,
  type TableEvent,
  type TableLogEntry,
} from "./table-events"
import type { RollRequest, TableRoll } from "./table-rolls"
import { EMPTY_TURNS, type TurnState } from "./turns"

export const STARTING_LIFE = 40
const TIMER_SYNC_MS = 15_000

interface MonarchEvent {
  holder: Monarch | null
  revision: number
}

/** The server's snapshot of durable table state, sent on join and after table changes. */
interface TableState {
  timer: GameTimerState
  peer_ids: string[]
  seats: TableParticipant[]
  turns: TurnState
  mode: GameFormat
  team_life: Record<number, number>
  monarch: MonarchEvent
  cards: BoardCard[]
}

type ErrorReply = { reason: string }

/** Seats, turns, timer, life, counters, monarch, rolls, and the table log: everything the
 * server tells the table about the game, and the commands a seat sends to change it. */
export function useTableGameState(
  link: RoomLink,
  playerId: number,
  setError: (error: string | null) => void,
) {
  const queryClient = useQueryClient()
  const monarchRevisionRef = useRef(-1)
  const [events, setEvents] = useState<TableEvent[]>([])
  const [participants, setParticipants] = useState<TableParticipant[]>([])
  const [eliminatedSeats, setEliminatedSeats] = useState<TableParticipant[]>([])
  const [seatOrder, setSeatOrder] = useState<string[]>([])
  const [shuffleVersion, setShuffleVersion] = useState(0)
  const [timer, setTimer] = useState<TimerSample | null>(null)
  const [turns, setTurns] = useState<TurnState>(EMPTY_TURNS)
  const [mode, setModeState] = useState<GameFormat>("commander")
  const [teamLife, setTeamLife] = useState<Record<number, number>>({})
  const [isOwner, setIsOwner] = useState(false)
  const [roll, setRoll] = useState<TableRoll | null>(null)
  const [monarch, setMonarch] = useState<Monarch | null>(null)
  // Your own life is tracked locally so rapid ± clicks compound before presence
  // echoes the new total back; presence stays the source for everyone else.
  const lifeRef = useRef(STARTING_LIFE)
  const [life, setLife] = useState(STARTING_LIFE)
  const countersRef = useRef(EMPTY_COUNTERS)
  const [counters, setCounters] = useState(EMPTY_COUNTERS)

  useEffect(() => {
    if (!roll) return
    const timeout = window.setTimeout(() => setRoll(null), 5000)
    return () => window.clearTimeout(timeout)
  }, [roll])

  const syncMonarch = useCallback(({ holder, revision }: MonarchEvent) => {
    if (revision < monarchRevisionRef.current) return
    monarchRevisionRef.current = revision
    setMonarch(holder)
  }, [])

  const receiveTimer = useCallback((state: GameTimerState) => {
    setTimer({ state, receivedAt: performance.now() })
  }, [])

  /** Re-anchors to server time so wall-clock changes and browser clock drift cannot accumulate. */
  const syncTimer = useCallback(() => {
    const channel = link.channel
    if (channel?.state !== "joined") return
    const sentAt = performance.now()
    channel.push("timer_sync", {}).receive("ok", (state: GameTimerState) => {
      if (link.channel === channel)
        setTimer({ state, receivedAt: (sentAt + performance.now()) / 2 })
    })
  }, [link])

  useEffect(() => {
    const interval = window.setInterval(syncTimer, TIMER_SYNC_MS)
    return () => window.clearInterval(interval)
  }, [syncTimer])

  const bindChannel = useCallback(
    (room: Channel) => {
      // The server owns the log: the full history on every (re)join, then each new or merged entry.
      room.on("table_log", ({ entries }: { entries: TableLogEntry[] }) =>
        setEvents(entries.map(toTableEvent)),
      )
      room.on("log_entry", (entry: TableLogEntry) =>
        setEvents((current) => receiveTableEvent(current, toTableEvent(entry))),
      )
      room.on("seat_order", ({ peer_ids, shuffled }: { peer_ids: string[]; shuffled: boolean }) => {
        setSeatOrder(peer_ids)
        if (shuffled) setShuffleVersion((version) => version + 1)
      })
      room.on("monarch_state", syncMonarch)
      room.on("monarch", syncMonarch)
      room.on("deck_selected", () => {
        void queryClient.invalidateQueries({ queryKey: ["decks"] })
      })
      room.on("table_state", (state: TableState) => {
        receiveTimer(state.timer)
        setSeatOrder(state.peer_ids)
        setEliminatedSeats(state.seats)
        setTurns(state.turns)
        setModeState(state.mode)
        setTeamLife(state.team_life)
        syncMonarch(state.monarch)
      })
      room.on(
        "eliminated_seats",
        ({ participants: eliminated }: { participants: TableParticipant[] }) =>
          setEliminatedSeats(eliminated),
      )
      room.on("timer_state", receiveTimer)
      room.on("roll", setRoll)
    },
    [queryClient, receiveTimer, syncMonarch],
  )

  /** Everyone present, from presence; spectators do not take seats. */
  const receivePresence = useCallback(
    (everyone: TableParticipant[]) => {
      const seats = everyone.filter((participant) => !participant.spectator)
      link.participants = seats
      setParticipants(seats)
    },
    [link],
  )

  /** Restores this seat from the server's copy on every (re)join, before any edits. */
  const hydrate = useCallback((participant: TableParticipant | undefined, owner: boolean) => {
    monarchRevisionRef.current = -1
    setIsOwner(owner)
    if (!participant) return
    lifeRef.current = participant.life
    setLife(participant.life)
    const restored = {
      poison: participant.poison,
      rad: participant.rad,
      commander_casts: participant.commander_casts,
      commander_damage: participant.commander_damage,
    }
    countersRef.current = restored
    setCounters(restored)
  }, [])

  /** Participants in shared seat order; the End game form records seats in this order. */
  const seatedParticipants = useMemo(
    () =>
      orderBySeats(retainEliminatedSeats(participants, eliminatedSeats), seatOrder).map(
        (liveParticipant) => {
          // Durable table state owns elimination, including offline teammates.
          const saved = eliminatedSeats.find((seat) => seat.player_id === liveParticipant.player_id)
          const participant = saved
            ? { ...liveParticipant, eliminated: saved.eliminated }
            : liveParticipant
          return participant.peer_id === link.peerId
            ? { ...participant, life, ...counters }
            : participant
        },
      ),
    [counters, eliminatedSeats, life, link, participants, seatOrder],
  )

  const reportError = ({ reason }: ErrorReply) => setError(reason)
  const canEditSeat = () => !link.spectator && link.channel?.state === "joined"

  const chooseDeck = useCallback(
    (deckId: number) => {
      link.channel?.push("choose_deck", { deck_id: deckId })
    },
    [link],
  )

  const updateStatus = useCallback(
    (changes: SeatStatus) => {
      link.channel?.push("update_status", changes)
    },
    [link],
  )

  function adjustTeamLife(teamIndex: number, delta: number) {
    link.channel
      ?.push("adjust_team_life", { team_index: teamIndex, delta })
      .receive("error", reportError)
  }

  function changeLife(delta: number) {
    if (!canEditSeat()) return
    if (mode === "two_headed_giant") {
      const index = seatedParticipants.findIndex((seat) => seat.player_id === playerId)
      if (index >= 0) adjustTeamLife(Math.floor(index / 2), delta)
      return
    }
    const next = Math.max(-999, Math.min(999, lifeRef.current + delta))
    lifeRef.current = next
    setLife(next)
    updateStatus({ life: next })
  }

  function adjustCounter(counter: Counter, delta: number) {
    if (!canEditSeat()) return
    const next = changeCounter(countersRef.current, counter, delta)
    countersRef.current = next
    setCounters(next)
    updateStatus(next)
  }

  function takeMonarch() {
    link.channel?.push("take_monarch", {})
  }

  /** Owner starts the match, either keeping the arranged order or shuffling it. */
  function startGame(randomize: boolean) {
    link.channel
      ?.push("start_game", { randomize })
      .receive("ok", () => setError(null))
      .receive("error", reportError)
  }

  const turnRevision = turns.revision
  const passTurn = useCallback(() => {
    link.channel
      ?.push("pass_turn", { revision: turnRevision })
      .receive("error", ({ reason }: ErrorReply) => setError(reason))
  }, [link, setError, turnRevision])

  const unpassTurn = useCallback(() => {
    link.channel
      ?.push("unpass_turn", { revision: turnRevision })
      .receive("error", ({ reason }: ErrorReply) => setError(reason))
  }, [link, setError, turnRevision])

  function adjustTurn(targetPlayerId: number, delta: -1 | 1) {
    link.channel
      ?.push("adjust_turn", { player_id: targetPlayerId, delta })
      .receive("error", reportError)
  }

  function setMode(nextMode: GameFormat) {
    link.channel
      ?.push("set_mode", { mode: nextMode })
      .receive("ok", () => setError(null))
      .receive("error", reportError)
  }

  /**
   * Owner swaps a seat with its neighbour. Before the match starts this only
   * rearranges; once started (Commander only) the server re-seats mid-game.
   */
  function moveSeat(peerId: string, delta: -1 | 1) {
    const peers = seatedParticipants.map((seat) => seat.peer_id)
    const index = peers.indexOf(peerId)
    const other = index + delta
    if (index < 0 || other < 0 || other >= peers.length) return
    ;[peers[index], peers[other]] = [peers[other]!, peers[index]!]
    link.channel
      ?.push(timer?.state.started_at == null ? "arrange_seats" : "seat_order", { peer_ids: peers })
      .receive("error", reportError)
  }

  function changeTimer(action: "pause" | "resume"): Promise<GameTimerState | null> {
    return new Promise((resolve) => {
      const channel = link.channel
      if (channel?.state !== "joined") {
        setError("Reconnect to the table before changing the timer")
        resolve(null)
        return
      }
      channel
        .push("timer", { action })
        .receive("ok", (state: GameTimerState) => {
          setTimer({ state, receivedAt: performance.now() })
          resolve(state)
        })
        .receive("error", ({ reason }: ErrorReply) => {
          setError(reason)
          resolve(null)
        })
        .receive("timeout", () => {
          setError("Timer request timed out; try again")
          resolve(null)
        })
    })
  }

  function setEliminated(peerId: string, eliminated: boolean) {
    link.channel
      ?.push("set_eliminated", { peer_id: peerId, eliminated })
      .receive("error", reportError)
      .receive("timeout", () => setError("Elimination request timed out; try again"))
  }

  function rollDice(request: RollRequest) {
    link.channel
      ?.push("roll", request)
      .receive("error", reportError)
      .receive("timeout", () =>
        setError("Roll request timed out; check the table log before retrying"),
      )
  }

  return {
    isOwner,
    participants: seatedParticipants,
    events,
    shuffleVersion,
    timer,
    turns,
    mode,
    teamLife,
    roll,
    monarch,
    life,
    counters,
    bindChannel,
    receivePresence,
    hydrate,
    syncTimer,
    chooseDeck,
    updateStatus,
    changeLife,
    adjustCounter,
    takeMonarch,
    startGame,
    passTurn,
    unpassTurn,
    adjustTurn,
    setMode,
    adjustTeamLife,
    moveSeat,
    changeTimer,
    setEliminated,
    rollDice,
  }
}
