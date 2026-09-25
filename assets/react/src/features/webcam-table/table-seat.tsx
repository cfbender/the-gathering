import { CameraTile } from "./board"
import { LifeControl } from "./life-control"
import type { TableParticipant } from "./room-types"
import { SeatBar } from "./seat-bar"
import { SeatCounterControls } from "./seat-counter-controls"
import {
  decksFor,
  isCurrentTurn,
  videoFlip,
  isLocal,
  revealLabels,
  streamFor,
  togglePin,
  type TableView,
} from "./table-view"
import { VideoStatsOverlay, type useVideoStats } from "./video-stats"

type SeatSize = "board" | "tile"

interface SeatProps {
  view: TableView
  participant: TableParticipant
  size: SeatSize
}

function SeatCounters({
  view,
  participant,
  onOpenChange,
}: Omit<SeatProps, "size"> & { onOpenChange: (open: boolean) => void }) {
  const { room } = view
  return (
    <SeatCounterControls
      participant={participant}
      participants={view.seated}
      decks={view.decks}
      local={isLocal(view, participant)}
      monarch={room.monarch?.peer_id === participant.peer_id}
      onAdjust={room.adjustCounter}
      onChangeLife={room.changeLife}
      onTakeMonarch={room.takeMonarch}
      onOpenChange={onOpenChange}
    />
  )
}

/** A seat's life total and counters; Two-Headed Giant shows counters only, life is shared. */
export function SeatLife({ view, participant, size }: SeatProps) {
  if (view.room.mode === "two_headed_giant")
    return (
      <div className="absolute top-2 left-2 w-14">
        <SeatCounters view={view} participant={participant} onOpenChange={() => {}} />
      </div>
    )
  return (
    <LifeControl
      life={participant.life}
      local={isLocal(view, participant)}
      size={size}
      counters={(onOpenChange) => (
        <SeatCounters view={view} participant={participant} onOpenChange={onOpenChange} />
      )}
      onChangeLife={view.room.changeLife}
    />
  )
}

/** The name bar under a seat's video with its commander and seat actions. */
export function SeatActions({ view, participant, size }: SeatProps) {
  const { room } = view
  return (
    <SeatBar
      participant={participant}
      local={isLocal(view, participant)}
      decks={decksFor(view, participant)}
      size={size}
      onChooseDeck={room.chooseDeck}
      onToggleCamera={view.toggleCamera}
      onReveal={view.openReveal}
      onSetEliminated={(eliminated) => room.setEliminated(participant.peer_id, eliminated)}
      flip={videoFlip(view, participant)}
      onToggleFlip={(axis) => view.preferences.toggleVideoFlip(participant.player_id, axis)}
      canEliminate={
        room.timer?.state.started_at != null &&
        !room.spectating &&
        (room.isOwner || isLocal(view, participant))
      }
    />
  )
}

/** A seat's camera tile with its name bar, for the camera rail and the grid. Clicking the video
 * pins that board; clicking it again releases it. */
export function SeatTile({
  view,
  participant,
  videoStats,
  fill = false,
}: {
  view: TableView
  participant: TableParticipant
  videoStats: ReturnType<typeof useVideoStats>
  fill?: boolean
}) {
  const { room, preferences } = view
  return (
    <div className={fill ? "flex min-h-0 flex-1 flex-col" : "overflow-hidden rounded-sm"}>
      <div className={fill ? "relative min-h-0 flex-1" : "relative"}>
        <CameraTile
          participant={participant}
          unattackable={view.protectedSeats.includes(participant.peer_id)}
          monarch={room.monarch?.peer_id === participant.peer_id}
          {...revealLabels(view, participant)}
          local={isLocal(view, participant)}
          flip={videoFlip(view, participant)}
          active={!view.showGrid && participant.peer_id === view.activeParticipant.peer_id}
          currentTurn={isCurrentTurn(view, participant)}
          connectionState={room.connectionStates[participant.peer_id]}
          stream={streamFor(view, participant)}
          fill={fill}
          onActivate={() => togglePin(view, participant)}
          lifeControl={<SeatLife view={view} participant={participant} size="tile" />}
        />
        {preferences.stats && (
          <VideoStatsOverlay
            stats={videoStats[participant.peer_id]}
            localStream={isLocal(view, participant) ? room.localStream : undefined}
          />
        )}
      </div>
      <SeatActions view={view} participant={participant} size="tile" />
    </div>
  )
}

/** Two-Headed Giant: the shared life total above a team's seats. */
export function TeamHeader({ view, group }: { view: TableView; group: TableParticipant[] }) {
  const { room } = view
  const index = view.groups.indexOf(group)
  return (
    <div
      className="relative flex h-20 items-center justify-end border-b border-primary/30 bg-base-200 px-3 text-xs text-base-content"
      aria-label={`Team ${index + 1} shared life`}
    >
      <LifeControl
        life={room.teamLife[index] ?? 60}
        size="tile"
        local={
          room.timer?.state.started_at != null &&
          !room.spectating &&
          (room.isOwner || group.some((seat) => seat.player_id === view.playerId))
        }
        counters={() => null}
        onChangeLife={(delta) => room.adjustTeamLife(index, delta)}
      />
      <span className="text-right">
        <strong className="block">Team {index + 1}</strong>
        <span className="text-base-content/60">Shared life</span>
      </span>
    </div>
  )
}
