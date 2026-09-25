import { cn } from "@/lib/cn"
import { CameraTile, OpenSeat } from "./board"
import { MAX_PLAYERS } from "./rooms"
import { SeatActions, SeatLife, TeamHeader } from "./table-seat"
import {
  isCurrentTurn,
  videoFlip,
  isLocal,
  revealLabels,
  streamFor,
  type TableView,
} from "./table-view"
import { VideoStatsOverlay, type useVideoStats } from "./video-stats"

/** Every seat's camera as a tile (grouped by team in Two-Headed Giant), plus open seats
 * before the match starts. Clicking a tile makes it the active board. */
export function TableCameraRail({
  view,
  videoStats,
}: {
  view: TableView
  videoStats: ReturnType<typeof useVideoStats>
}) {
  const { room, preferences } = view
  const teamsMode = room.mode === "two_headed_giant"
  const openSeats =
    room.timer?.state.started_at != null ? 0 : Math.max(0, MAX_PLAYERS - view.seated.length)

  return (
    <aside
      className={cn(
        "flex min-h-0 gap-1.5 overflow-x-auto p-1.5 lg:flex-col lg:overflow-x-hidden lg:overflow-y-auto",
        preferences.panelLeft && "lg:order-5",
      )}
      aria-label="Player cameras"
    >
      {view.groups.map((group) => (
        <div
          key={group[0]!.peer_id}
          className={cn(
            "w-60 shrink-0 lg:w-auto",
            teamsMode && "overflow-hidden rounded-lg border border-primary/40",
          )}
        >
          {teamsMode && <TeamHeader view={view} group={group} />}
          {group.map((participant) => (
            <div key={participant.peer_id} className="overflow-hidden rounded-sm">
              <div className="relative">
                <CameraTile
                  participant={participant}
                  unattackable={view.protectedSeats.includes(participant.peer_id)}
                  monarch={room.monarch?.peer_id === participant.peer_id}
                  {...revealLabels(view, participant)}
                  local={isLocal(view, participant)}
                  flip={videoFlip(view, participant)}
                  active={participant.peer_id === view.activeParticipant.peer_id}
                  currentTurn={isCurrentTurn(view, participant)}
                  connectionState={room.connectionStates[participant.peer_id]}
                  stream={streamFor(view, participant)}
                  onActivate={() => view.selectBoard(participant.peer_id)}
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
          ))}
        </div>
      ))}
      {Array.from({ length: openSeats }, (_, index) => (
        <div key={`open-${index}`} className="hidden w-44 shrink-0 lg:block lg:w-auto">
          <OpenSeat />
        </div>
      ))}
    </aside>
  )
}
