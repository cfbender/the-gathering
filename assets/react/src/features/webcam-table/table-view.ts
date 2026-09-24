import type { DeckSummary } from "@/features/decks/decks"
import { teams, turnId, unattackableSeats } from "./game-modes"
import { canViewBoard } from "./media-policy"
import type { TableParticipant } from "./room-types"
import type { useTablePreferences } from "./table-preferences"
import { useActiveBoard } from "./use-active-board"
import type { useWebcamRoom } from "./use-webcam-room"

export type WebcamRoom = ReturnType<typeof useWebcamRoom>
export type TablePreferences = ReturnType<typeof useTablePreferences>

/** The table as the camera rail and stage draw it: who is seated, how seats group into
 * teams, which board is active, and the room/preferences behind every seat control. */
export interface TableView {
  room: WebcamRoom
  preferences: TablePreferences
  playerId: number
  decks: DeckSummary[]
  seated: TableParticipant[]
  groups: TableParticipant[][]
  localParticipant: TableParticipant
  activeParticipant: TableParticipant
  activeGroup: TableParticipant[]
  /** Five Star: seats the viewer may not attack yet. */
  protectedSeats: string[]
  boardPinned: boolean
  selectBoard: (peerId: string) => void
  toggleBoardPin: () => void
  toggleCamera: () => void
  openReveal: () => void
}

export function useTableView({
  room,
  preferences,
  playerId,
  playerName,
  decks,
  toggleCamera,
  openReveal,
}: Pick<
  TableView,
  "room" | "preferences" | "playerId" | "decks" | "toggleCamera" | "openReveal"
> & { playerName: string }): TableView {
  const board = useActiveBoard(room.participants, room.peerId)
  const localParticipant: TableParticipant = room.participants.find(
    (participant) => participant.peer_id === room.peerId,
  ) ?? {
    peer_id: room.peerId,
    player_id: playerId,
    player_name: playerName,
    life: room.life,
    ...room.counters,
    camera_off: room.cameraOff,
    eliminated: false,
    joined_at: Number.MAX_SAFE_INTEGER,
  }
  const seated =
    room.spectating || room.participants.some((participant) => participant.peer_id === room.peerId)
      ? room.participants
      : [localParticipant, ...room.participants]
  // A pinned board overrides following the turn without changing the saved view mode.
  const activeParticipant =
    (preferences.followTurn &&
      !board.pinned &&
      seated.find((participant) => participant.player_id === room.turns.active_player_id)) ||
    seated.find((participant) => participant.peer_id === board.selectedPeerId) ||
    (room.spectating && seated[0]) ||
    localParticipant
  const groups = room.mode === "two_headed_giant" ? teams(seated) : seated.map((seat) => [seat])
  return {
    room,
    preferences,
    playerId,
    decks,
    seated,
    groups,
    localParticipant,
    activeParticipant,
    activeGroup: groups.find((group) => group.includes(activeParticipant)) ?? [activeParticipant],
    protectedSeats:
      room.mode === "five_star" && !room.spectating ? unattackableSeats(seated, playerId) : [],
    boardPinned: board.pinned,
    selectBoard: board.select,
    toggleBoardPin: board.togglePin,
    toggleCamera,
    openReveal,
  }
}

export function isLocal(view: TableView, participant: TableParticipant) {
  return participant.peer_id === view.room.peerId
}

export function decksFor(view: TableView, participant: TableParticipant) {
  return view.decks.filter((deck) => deck.player_id === participant.player_id)
}

export function isFlipped(view: TableView, participant: TableParticipant) {
  return (
    !isLocal(view, participant) && view.preferences.flippedPlayerIds.includes(participant.player_id)
  )
}

export function isPinned(view: TableView, participant: TableParticipant) {
  return view.boardPinned && participant.peer_id === view.activeParticipant.peer_id
}

export function togglePin(view: TableView, participant: TableParticipant) {
  if (isPinned(view, participant)) view.toggleBoardPin()
  else view.selectBoard(participant.peer_id)
}

export function isCurrentTurn(view: TableView, participant: TableParticipant) {
  return (
    turnId(view.seated, participant.player_id, view.room.mode) === view.room.turns.active_player_id
  )
}

export function streamFor(view: TableView, participant: TableParticipant) {
  return isLocal(view, participant)
    ? (view.room.localStream ?? undefined)
    : view.room.streams[participant.peer_id]
}

/** Labels for a board hidden by, or revealed to you through, a private reveal. */
export function revealLabels(view: TableView, participant: TableParticipant) {
  const target = view.seated.find((seat) => seat.peer_id === participant.reveal_to)
  return {
    hiddenLabel: canViewBoard(participant.peer_id, view.room.peerId, participant.reveal_to)
      ? undefined
      : `Revealing to ${target?.player_name ?? "another player"}`,
    revealBadge:
      participant.reveal_to === view.room.peerId
        ? `${participant.player_name} is revealing to you`
        : undefined,
  }
}
