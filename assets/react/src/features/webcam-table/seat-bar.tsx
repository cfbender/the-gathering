import {
  Ellipsis,
  Eye,
  LayoutDashboard,
  TrianglesCenterlineDashedHorizontal,
  TrianglesCenterlineDashedVertical,
  Video,
  VideoOff,
} from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { NO_FLIP, type FlipAxis, type VideoFlip } from "./board"
import { commanderBackground } from "./commander-colors"
import { CommanderControl } from "./commander-control"
import type { TableParticipant } from "./use-webcam-room"

/** This seat's virtual board controls, offered on your own seat bar. */
export interface SeatBoardControls {
  active: boolean
  /** Why it cannot start, or null. */
  unavailable: string | null
  onStart: () => void
  onStop: () => void
  onOpen: () => void
}

// Each icon shows the mirror line: a vertical flip mirrors across a horizontal line.
const FLIP_ITEMS = [
  { axis: "vertical", Icon: TrianglesCenterlineDashedHorizontal },
  { axis: "horizontal", Icon: TrianglesCenterlineDashedVertical },
] as const

/** One line below either video: identity, actions, camera state, commander picker. */
export function SeatBar({
  participant,
  local,
  decks,
  size,
  onChooseDeck,
  onToggleCamera,
  onReveal,
  onSetEliminated,
  flip = NO_FLIP,
  onToggleFlip,
  canEliminate = local,
  virtualBoard,
}: {
  participant: TableParticipant
  local: boolean
  decks: DeckSummary[]
  size: "board" | "tile"
  onChooseDeck: (deckId: number) => void
  onToggleCamera: () => void
  onReveal: () => void
  onSetEliminated: (eliminated: boolean) => void
  flip?: VideoFlip
  onToggleFlip?: (axis: FlipAxis) => void
  canEliminate?: boolean
  virtualBoard?: SeatBoardControls
}) {
  const compact = size === "tile"
  const cameraIcon = participant.camera_off ? (
    <VideoOff className="size-3.5" />
  ) : (
    <Video className="size-3.5" />
  )

  return (
    <div
      style={{
        background: commanderBackground(
          decks.find((deck) => deck.id === participant.deck_id)?.color_identity ?? "",
        ),
      }}
      className={cn(
        "flex min-w-0 items-center gap-0.5 border-t border-white/10 bg-base-100 text-white",
        compact ? "h-9 px-1 text-[0.7rem]" : "h-11 px-2 text-xs",
      )}
    >
      <span
        className={cn(
          "min-w-0 truncate font-bold",
          // The name keeps its full text (up to a cap); the commander yields and truncates first.
          compact ? "max-w-[45%] shrink-0" : "max-w-48",
        )}
        title={`${participant.player_name}${local ? " (you)" : ""}`}
      >
        {participant.player_name}
      </span>
      {local && !compact && (
        <span className="rounded bg-white/15 px-1 text-[0.6rem] font-bold">YOU</span>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="btn btn-ghost btn-sm btn-square shrink-0 text-white/85 focus-visible:-outline-offset-2"
            aria-label={`${participant.player_name}'s seat actions`}
          >
            <Ellipsis className="size-4" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          {onToggleFlip &&
            FLIP_ITEMS.map(({ axis, Icon }) => (
              <DropdownMenuItem
                key={axis}
                onSelect={() => onToggleFlip(axis)}
                aria-label={`${flip[axis] ? "Unflip" : "Flip"} ${participant.player_name}'s video ${axis}ly`}
                title={
                  local
                    ? "Only changes your preview; others see your camera as sent"
                    : "Only changes your view; remembered for this player"
                }
              >
                <Icon className="size-4" />
                {`${flip[axis] ? "Unflip" : "Flip"} video ${axis}ly`}
              </DropdownMenuItem>
            ))}
          {virtualBoard &&
            (virtualBoard.active ? (
              <DropdownMenuItem onSelect={virtualBoard.onStop}>
                <Video className="size-4" /> Use camera instead
              </DropdownMenuItem>
            ) : (
              <DropdownMenuItem
                onSelect={virtualBoard.onStart}
                disabled={virtualBoard.unavailable !== null}
                title={virtualBoard.unavailable ?? "Play on a virtual tabletop instead of a camera"}
              >
                <LayoutDashboard className="size-4" />
                <span>
                  Use virtual board
                  {virtualBoard.unavailable && (
                    <span className="block text-xs font-normal text-base-content/60">
                      {virtualBoard.unavailable}
                    </span>
                  )}
                </span>
              </DropdownMenuItem>
            ))}
          {local && (
            <>
              {!virtualBoard?.active && (
                <DropdownMenuItem onSelect={onToggleCamera}>
                  {cameraIcon}
                  {participant.camera_off ? "Turn camera on" : "Turn camera off"}
                </DropdownMenuItem>
              )}
              <DropdownMenuItem onSelect={onReveal}>
                <Eye className="size-4" /> Reveal hand…
              </DropdownMenuItem>
            </>
          )}
          {canEliminate && (
            <DropdownMenuItem
              destructive={!participant.eliminated}
              onSelect={() => onSetEliminated(!participant.eliminated)}
            >
              {participant.eliminated ? "Restore player" : "Eliminate player"}
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      {local && virtualBoard?.active ? (
        <button
          type="button"
          // Phones open it from the thumb bar.
          className="btn btn-primary btn-sm shrink-0 gap-1 px-2 focus-visible:-outline-offset-2 max-lg:hidden"
          onClick={virtualBoard.onOpen}
          title="Open your virtual board"
        >
          <LayoutDashboard className="size-3.5" />
          {compact ? <span className="sr-only">My board</span> : "My board"}
        </button>
      ) : local ? (
        <button
          type="button"
          className={cn(
            "btn btn-ghost btn-sm btn-square shrink-0 focus-visible:-outline-offset-2",
            participant.camera_off ? "text-error" : "text-white/85",
          )}
          onClick={onToggleCamera}
          aria-pressed={participant.camera_off}
          aria-label={participant.camera_off ? "Turn camera on" : "Turn camera off"}
          title={participant.camera_off ? "Turn camera on" : "Turn camera off"}
        >
          {cameraIcon}
        </button>
      ) : (
        <span
          className={cn(
            "grid size-8 shrink-0 place-items-center",
            participant.camera_off ? "text-error" : "text-white/55",
          )}
          role="img"
          aria-label={participant.camera_off ? "Camera off" : "Camera on"}
        >
          {cameraIcon}
        </span>
      )}
      <div
        className={cn("ml-auto flex min-w-0 justify-end", compact ? "pl-1" : "max-w-[50%] pl-2")}
      >
        <CommanderControl
          participant={participant}
          decks={decks}
          local={local}
          compact={compact}
          onChooseDeck={onChooseDeck}
        />
      </div>
    </div>
  )
}
