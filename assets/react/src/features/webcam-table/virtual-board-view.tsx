import {
  DeckPlaytester,
  PlaytestSpectatorBoard,
  type PlaytestCard,
  type PublicPlaytestSnapshot,
} from "@cfbender/playtest"
import { ArrowLeft, SkipForward } from "lucide-react"
import { manaSymbolUrl } from "@/components/mana-symbols"
import { cn } from "@/lib/cn"
import type { IdentifiedCard } from "./room-types"
import { isCurrentTurn, seatLife, type TableView } from "./table-view"

const SLEEVE = "/images/logo.svg"
const PRINTING_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

/** A board card the card preview can open: dealt cards carry their printing (or the catalog
 * card) in `deckCardId`; tokens and unknown names have none. */
function previewable(card: PlaytestCard): IdentifiedCard | null {
  return PRINTING_ID.test(card.deckCardId)
    ? { id: card.deckCardId, name: card.name, set: "" }
    : null
}

/** A seat's virtual board as the table sees it, in place of its camera. */
export function SeatBoard({
  board,
  onPreview,
  className,
}: {
  board: PublicPlaytestSnapshot
  /** Tapping a face-up card opens it; omitted where a tap selects the seat instead. */
  onPreview?: (card: IdentifiedCard) => void
  className?: string
}) {
  return (
    <PlaytestSpectatorBoard
      snapshot={board}
      showPlayerStats={false}
      sleeveImageUrl={SLEEVE}
      manaSymbolUrl={manaSymbolUrl}
      className={className}
      onCardPreview={
        onPreview
          ? (card) => {
              const shown = previewable(card)
              if (shown) onPreview(shown)
            }
          : undefined
      }
    />
  )
}

/** The seat's own virtual board, full screen: the shared playtester with the table's life, a
 * way back to the table, and Pass turn. */
export function VirtualBoardScreen({ view, onClose }: { view: TableView; onClose: () => void }) {
  const { room, virtualBoard, localParticipant } = view
  const saved = virtualBoard.saved
  if (!saved) return null
  const life = seatLife(view, localParticipant)
  const started = room.timer?.state.started_at != null
  const myTurn = isCurrentTurn(view, localParticipant)
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Your virtual board"
      className="fixed inset-0 z-50 h-dvh bg-base-100 text-base-content"
    >
      <DeckPlaytester
        // The table watches this game; a rematch deals a fresh one.
        canRestart={false}
        title={saved.deckName}
        initialState={saved.initialState}
        snapshot={saved.snapshot}
        onSnapshotChange={virtualBoard.change}
        life={{ value: life, onChange: (next) => room.changeLife(next - life) }}
        playerCounters={null}
        turn={null}
        sleeveImageUrl={SLEEVE}
        manaSymbolUrl={manaSymbolUrl}
        settingsKey="the-gathering:playtest-settings"
        closeSlot={
          <>
            <button type="button" className="btn btn-ghost btn-sm gap-1.5 px-2" onClick={onClose}>
              <ArrowLeft className="size-4" />
              <span className="max-sm:sr-only">Table</span>
            </button>
            {started && !room.spectating && (
              <button
                type="button"
                className={cn(
                  "btn btn-sm gap-1.5 px-2.5 whitespace-nowrap",
                  myTurn ? "btn-primary" : "btn-outline",
                )}
                onClick={room.passTurn}
                disabled={room.turns.active_player_id === null}
              >
                <SkipForward className="size-4" />
                <span className="max-sm:sr-only">Pass turn</span>
              </button>
            )}
          </>
        }
      />
    </div>
  )
}
