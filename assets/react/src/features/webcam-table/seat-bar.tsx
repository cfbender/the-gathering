import { Ellipsis, Eye, Pin, PinOff, Video, VideoOff } from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { commanderBackground } from "./commander-colors"
import { CommanderControl } from "./commander-control"
import type { TableParticipant } from "./use-webcam-room"

/** One line below either video: identity, actions, camera state, commander picker. */
export function SeatBar({
  participant,
  local,
  decks,
  size,
  pinned,
  onChooseDeck,
  onToggleCamera,
  onReveal,
  onTogglePin,
  onSetEliminated,
}: {
  participant: TableParticipant
  local: boolean
  decks: DeckSummary[]
  size: "board" | "tile"
  pinned: boolean
  onChooseDeck: (deckId: number) => void
  onToggleCamera: () => void
  onReveal: () => void
  onTogglePin: () => void
  onSetEliminated: (eliminated: boolean) => void
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
        className={cn("min-w-0 truncate font-bold", compact ? "max-w-[30%]" : "max-w-48")}
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
          <DropdownMenuItem onSelect={onTogglePin}>
            {pinned ? <PinOff className="size-4" /> : <Pin className="size-4" />}
            {pinned ? "Unpin board" : "Pin as active board"}
          </DropdownMenuItem>
          {local && (
            <>
              <DropdownMenuItem onSelect={onToggleCamera}>
                {cameraIcon}
                {participant.camera_off ? "Turn camera on" : "Turn camera off"}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={onReveal}>
                <Eye className="size-4" /> Reveal hand…
              </DropdownMenuItem>
            </>
          )}
          <DropdownMenuItem
            destructive={!participant.eliminated}
            onSelect={() => onSetEliminated(!participant.eliminated)}
          >
            {participant.eliminated ? "Restore player" : "Eliminate player"}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <span
        className={cn("shrink-0", participant.camera_off ? "text-error" : "text-white/55")}
        role="img"
        aria-label={participant.camera_off ? "Camera off" : "Camera on"}
      >
        {cameraIcon}
      </span>
      <div className={cn("ml-auto min-w-0", compact ? "flex-1 pl-1" : "max-w-[50%] pl-2")}>
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
