import { Link } from "@tanstack/react-router"
import { cn } from "@/lib/cn"
import { ActiveBoard, capturePoint } from "./board"
import { BoardCardTray } from "./board-cards"
import { CardPreview } from "./card-preview"
import { CardSuggestions } from "./card-suggestions"
import type { TableParticipant } from "./room-types"
import { describeRoll } from "./table-rolls"
import { SeatActions, SeatLife, TeamHeader } from "./table-seat"
import {
  isCurrentTurn,
  videoFlip,
  isLocal,
  isPinned,
  revealLabels,
  streamFor,
  togglePin,
  type TableView,
} from "./table-view"
import type { CardIdentificationFlow } from "./use-card-identification-flow"

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
          pinned={isPinned(view, participant)}
          onTogglePin={() => togglePin(view, participant)}
          onInspect={(event) => {
            const point = capturePoint(event, videoFlip(view, participant))
            if (point) room.requestCapture(participant.peer_id, point.x, point.y, event.shiftKey)
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
          onSearch={recognizer.search}
          onPrintings={recognizer.printings}
          galleryVersion={"version" in recognizer.state ? recognizer.state.version : undefined}
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

/** The active board (or team) filling the middle of the table. */
export function TableStage({ view, flow }: { view: TableView; flow: CardIdentificationFlow }) {
  const { room } = view
  return (
    <section
      className={cn(
        "relative flex min-h-0 min-w-0 flex-col",
        view.preferences.panelLeft && "lg:order-3",
      )}
      aria-label="Active board"
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
      {room.mode === "two_headed_giant" && <TeamHeader view={view} group={view.activeGroup} />}
      <div className="relative flex min-h-0 flex-1 flex-col">
        {view.activeGroup.map((participant) => (
          <StageBoard key={participant.peer_id} view={view} participant={participant} flow={flow} />
        ))}
        <IdentificationOverlays view={view} flow={flow} />
      </div>
    </section>
  )
}
