import { Link } from "@tanstack/react-router"
import { LayoutGrid, Undo2 } from "lucide-react"
import { cn } from "@/lib/cn"
import { ActiveBoard, capturePoint } from "./board"
import { BoardCardTray } from "./board-cards"
import { CardPreview } from "./card-preview"
import { CardSuggestions } from "./card-suggestions"
import type { TableParticipant } from "./room-types"
import { describeRoll } from "./table-rolls"
import { SeatActions, SeatLife, SeatTile, TeamHeader } from "./table-seat"
import {
  isCurrentTurn,
  videoFlip,
  isLocal,
  isPinned,
  revealLabels,
  streamFor,
  type TableView,
} from "./table-view"
import type { CardIdentificationFlow } from "./use-card-identification-flow"
import type { useVideoStats } from "./video-stats"

function StageBoard({
  view,
  participant,
  flow,
}: {
  view: TableView
  participant: TableParticipant
  flow: CardIdentificationFlow
}) {
  const { room } = view
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="relative min-h-0 flex-1">
        <ActiveBoard
          participant={participant}
          unattackable={view.protectedSeats.includes(participant.peer_id)}
          monarch={room.monarch?.peer_id === participant.peer_id}
          {...revealLabels(view, participant)}
          local={isLocal(view, participant)}
          flip={videoFlip(view, participant)}
          currentTurn={isCurrentTurn(view, participant)}
          connectionState={room.connectionStates[participant.peer_id]}
          stream={streamFor(view, participant)}
          lifeControl={<SeatLife view={view} participant={participant} size="board" />}
          release={
            !isPinned(view, participant)
              ? undefined
              : view.preferences.viewMode === "grid"
                ? {
                    label: "Back to grid",
                    title: "Show every camera again (or click this player's camera)",
                    icon: LayoutGrid,
                    onClick: view.releaseBoard,
                  }
                : {
                    label: "Follow turn",
                    title: "Go back to following the active turn (or click this player's camera)",
                    icon: Undo2,
                    onClick: view.releaseBoard,
                  }
          }
          onInspect={(event) => {
            const flip = videoFlip(view, participant)
            const point = capturePoint(event, flip)
            if (point)
              room.requestCapture(participant.peer_id, point.x, point.y, event.shiftKey, flip)
          }}
        />
        <BoardCardTray
          participant={participant}
          cards={room.identifiedCards}
          onPreview={flow.previewEntry}
          onRemove={room.removeCard}
          onClear={
            participant.peer_id === view.localParticipant.peer_id ? room.clearOwnCards : undefined
          }
        />
      </div>
      <SeatActions view={view} participant={participant} size="board" />
    </div>
  )
}

/** Grid view: every seat's camera shares the stage (teams stay together in Two-Headed Giant).
 * Clicking one fills the stage with that board until it is clicked again. */
function CameraGrid({
  view,
  videoStats,
}: {
  view: TableView
  videoStats: ReturnType<typeof useVideoStats>
}) {
  const teamsMode = view.room.mode === "two_headed_giant"
  const columns = Math.ceil(Math.sqrt(view.groups.length))
  const rows = Math.ceil(view.groups.length / columns)
  return (
    <div
      className="grid min-h-0 flex-1 gap-1.5 p-1.5"
      style={{
        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
        gridTemplateRows: `repeat(${rows}, minmax(0, 1fr))`,
      }}
    >
      {view.groups.map((group) => (
        <div
          key={group[0]!.peer_id}
          className={cn(
            "flex min-h-0 min-w-0 flex-col overflow-hidden rounded-sm",
            teamsMode && "rounded-lg border border-primary/40",
          )}
        >
          {teamsMode && <TeamHeader view={view} group={group} />}
          {group.map((participant) => (
            <SeatTile
              key={participant.peer_id}
              view={view}
              participant={participant}
              videoStats={videoStats}
              fill
            />
          ))}
        </div>
      ))}
    </div>
  )
}

/** Card identification on top of the active board: progress, the picker, and the preview. */
function IdentificationOverlays({ view, flow }: { view: TableView; flow: CardIdentificationFlow }) {
  const { capture, captureOwner, recognition, recognizer, preview } = flow
  return (
    <>
      {capture && recognition.status === "identifying" && !flow.pickerOpen && !preview && (
        <p
          role="status"
          className="absolute bottom-12 left-1/2 z-10 -translate-x-1/2 rounded-lg bg-base-100/95 px-4 py-2 text-sm text-base-content shadow-lg"
        >
          {recognition.loading ? "Loading card scanner…" : "Identifying card…"}
        </p>
      )}
      {capture && captureOwner && flow.pickerOpen && (
        <CardSuggestions
          capture={capture}
          playerName={captureOwner.player_name}
          recognition={recognition}
          deckSuggestions={flow.suggestions}
          gallerySearchable
          onChooseCard={flow.chooseCard}
          onChooseDeck={flow.chooseDeckForCapture}
          onSearch={flow.search}
          onPrintings={recognizer.printings}
          galleryVersion={"version" in recognizer.state ? recognizer.state.version : undefined}
          deckNames={flow.ownerDeckNames}
          onDismiss={flow.dismissPicker}
        />
      )}
      {preview?.kind === "entry" && (
        <CardPreview
          card={preview.shown}
          ownerName={
            view.seated.find((seat) => seat.peer_id === preview.entry.ownerPeerId)?.player_name
          }
          onWrongCard={
            preview.correctable && capture ? () => flow.correctPreview(preview.entry.id) : undefined
          }
          onRemove={() => {
            view.room.removeCard(preview.entry.id)
            flow.closePreview()
          }}
          onClose={flow.closePreview}
        />
      )}
      {preview?.kind === "art" && <CardPreview card={preview.card} onClose={flow.closePreview} />}
    </>
  )
}

/** The middle of the table: the active board (or team), or every camera in grid view. */
export function TableStage({
  view,
  flow,
  videoStats,
}: {
  view: TableView
  flow: CardIdentificationFlow
  videoStats: ReturnType<typeof useVideoStats>
}) {
  const { room } = view
  return (
    <section
      className={cn(
        "relative flex min-h-0 min-w-0 flex-col",
        view.preferences.panelLeft && "lg:order-3",
      )}
      aria-label={view.showGrid ? "Camera grid" : "Active board"}
    >
      {room.spectating && (
        <p role="status" className="bg-base-200 px-4 py-2 text-sm font-semibold text-base-content">
          Spectating — this game has already started. Your camera is not shared.
          <Link to="/games" className="link ml-3">
            Leave table
          </Link>
        </p>
      )}
      {room.roll && (
        <div
          role="status"
          className="pointer-events-none absolute top-20 left-1/2 z-30 w-max max-w-[90%] -translate-x-1/2 rounded-xl border border-accent/40 bg-base-100/95 px-6 py-4 text-center text-lg font-semibold text-base-content shadow-xl"
        >
          {describeRoll(room.roll)}
        </div>
      )}
      {room.mode === "two_headed_giant" && !view.showGrid && (
        <TeamHeader view={view} group={view.activeGroup} />
      )}
      <div className="relative flex min-h-0 flex-1 flex-col">
        {view.showGrid ? (
          <CameraGrid view={view} videoStats={videoStats} />
        ) : (
          view.activeGroup.map((participant) => (
            <StageBoard
              key={participant.peer_id}
              view={view}
              participant={participant}
              flow={flow}
            />
          ))
        )}
        <IdentificationOverlays view={view} flow={flow} />
      </div>
    </section>
  )
}
