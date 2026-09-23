import { Heart, Minus, Plus, Video, VideoOff } from "lucide-react"
import type { ButtonHTMLAttributes, ReactNode } from "react"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { commanderBackground } from "./commander-colors"
import { CommanderTax } from "./commander-tax"
import type { Counter } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

interface Props {
  participant: TableParticipant
  local: boolean
  decks: DeckSummary[]
  size: "board" | "tile"
  onChooseDeck: (deckId: number) => void
  onChangeLife: (delta: number) => void
  onToggleCamera: () => void
  counters: ReactNode
  onAdjustCounter: (counter: Counter, delta: number) => void
}

function IndicatorButton({ className, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className={cn(
        "grid size-6 place-items-center rounded text-white/75 transition hover:bg-white/10 hover:text-white disabled:cursor-not-allowed disabled:text-white/35 disabled:hover:bg-transparent",
        className,
      )}
      {...props}
    />
  )
}

/** Name bar under a board or tile: name, life (with ± for your own seat), the camera
 * indicator, and the seat's commander action on the right. */
export function SeatBar({
  participant,
  local,
  decks,
  size,
  onChooseDeck,
  onChangeLife,
  onToggleCamera,
  counters,
  onAdjustCounter,
}: Props) {
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
        "flex items-center gap-1.5 border-t border-white/10 bg-base-100 text-white",
        compact ? "h-7 px-1.5 text-[0.7rem]" : "min-h-11 flex-wrap px-2 py-1 text-xs",
      )}
    >
      <span className={cn("truncate font-bold", compact ? "max-w-24" : "max-w-48")}>
        {participant.player_name}
      </span>
      {local && <span className="rounded bg-white/15 px-1 text-[0.6rem] font-bold">YOU</span>}

      <span className="ml-1 flex items-center gap-0.5 text-white/85" aria-label="Life total">
        <Heart className="size-3 fill-current text-error" />
        <span className="font-bold tabular-nums">{participant.life}</span>
      </span>
      {local && (
        <span className="flex items-center gap-0.5">
          <IndicatorButton aria-label="Lose 1 life" onClick={() => onChangeLife(-1)}>
            <Minus className="size-3.5" />
          </IndicatorButton>
          <IndicatorButton aria-label="Gain 1 life" onClick={() => onChangeLife(1)}>
            <Plus className="size-3.5" />
          </IndicatorButton>
        </span>
      )}

      {!compact && counters}
      <span className="ml-auto flex items-center gap-0.5">
        {local ? (
          <IndicatorButton
            onClick={onToggleCamera}
            aria-pressed={participant.camera_off}
            aria-label={participant.camera_off ? "Turn camera on" : "Turn camera off"}
            title={participant.camera_off ? "Turn camera on" : "Turn camera off"}
          >
            {cameraIcon}
          </IndicatorButton>
        ) : (
          <span
            className={cn(
              "grid size-6 place-items-center",
              participant.camera_off ? "text-error" : "text-white/55",
            )}
            title={participant.camera_off ? "Camera off" : "Camera on"}
          >
            {cameraIcon}
          </span>
        )}
      </span>

      {!compact && (
        <div className="max-w-full min-w-0 sm:max-w-[50%]">
          <CommanderTax
            participant={participant}
            decks={decks}
            local={local}
            onChooseDeck={onChooseDeck}
            onAdjust={onAdjustCounter}
          />
        </div>
      )}
    </div>
  )
}

/** Tile variant of the commander action, rendered under a rail tile's bar. */
export function TileCommanderRow({
  participant,
  decks,
  onChooseDeck,
  counters,
  local,
  onAdjustCounter,
}: Pick<
  Props,
  "participant" | "decks" | "onChooseDeck" | "counters" | "local" | "onAdjustCounter"
>) {
  return (
    <div
      className="flex min-h-10 items-center gap-1 bg-base-100 px-1.5 py-1"
      style={{
        background: commanderBackground(
          decks.find((deck) => deck.id === participant.deck_id)?.color_identity ?? "",
        ),
      }}
    >
      {counters}
      <div className="ml-auto min-w-0">
        <CommanderTax
          participant={participant}
          decks={decks}
          local={local}
          onChooseDeck={onChooseDeck}
          onAdjust={onAdjustCounter}
        />
      </div>
    </div>
  )
}
