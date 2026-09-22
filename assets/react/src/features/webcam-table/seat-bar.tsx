import { Heart, MicOff, Minus, Plus, Video, VideoOff } from "lucide-react"
import type { ButtonHTMLAttributes } from "react"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderPicker } from "./commander-picker"
import type { TableParticipant } from "./use-webcam-room"

interface Props {
  participant: TableParticipant
  local: boolean
  decks: DeckSummary[]
  size: "board" | "tile"
  onChooseDeck: (deckId: number) => void
  onChangeLife: (delta: number) => void
  onToggleCamera: () => void
}

const MIC_TITLE = "Voice chat is not part of the webcam table yet; use your usual voice app"

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

/** Name bar under a board or tile: name, life (with ± for your own seat), mic/camera
 * indicators, and the seat's commander action on the right. */
export function SeatBar({
  participant,
  local,
  decks,
  size,
  onChooseDeck,
  onChangeLife,
  onToggleCamera,
}: Props) {
  const compact = size === "tile"
  const cameraIcon = participant.camera_off ? (
    <VideoOff className="size-3.5" />
  ) : (
    <Video className="size-3.5" />
  )

  return (
    <div
      className={cn(
        "flex items-center gap-1.5 border-t border-white/10 bg-base-100 text-white",
        compact ? "h-7 px-1.5 text-[0.7rem]" : "h-9 px-2 text-xs",
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

      <span className="ml-auto flex items-center gap-0.5">
        <IndicatorButton disabled title={MIC_TITLE} aria-label="Microphone unavailable">
          <MicOff className="size-3.5" />
        </IndicatorButton>
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
        <CommanderPicker
          playerName={participant.player_name}
          decks={decks}
          selectedDeckId={participant.deck_id}
          onChoose={onChooseDeck}
        />
      )}
    </div>
  )
}

/** Tile variant of the commander action, rendered under a rail tile's bar. */
export function TileCommanderRow({
  participant,
  decks,
  onChooseDeck,
}: Pick<Props, "participant" | "decks" | "onChooseDeck">) {
  return (
    <div className="flex h-6 items-center justify-end bg-base-100 px-1.5">
      <CommanderPicker
        playerName={participant.player_name}
        decks={decks}
        selectedDeckId={participant.deck_id}
        onChoose={onChooseDeck}
      />
    </div>
  )
}
