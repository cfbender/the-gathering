import { cn } from "@/lib/cn"
import { OpenSeat } from "./board"
import { MAX_PLAYERS } from "./rooms"
import { SeatTile, TeamHeader } from "./table-seat"
import type { TableView } from "./table-view"
import type { useVideoStats } from "./video-stats"

/** Every seat's camera as a tile (grouped by team in Two-Headed Giant), plus open seats
 * before the match starts. Clicking a tile pins it as the active board; clicking again releases. */
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
            <SeatTile
              key={participant.peer_id}
              view={view}
              participant={participant}
              videoStats={videoStats}
            />
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
