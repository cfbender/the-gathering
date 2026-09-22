import { ArrowDown, ArrowUp, Trash2, Trophy } from "lucide-react"
import type { ReactNode } from "react"
import type { DraftSeat } from "@/features/games/use-game-draft"

interface SeatEditorProps {
  seat: DraftSeat
  index: number
  seatCount: number
  winner: boolean
  onChooseWinner: () => void
  onMove: (direction: -1 | 1) => void
  onRemove: () => void
  canRemove?: boolean
  children: ReactNode
}

export function SeatEditor({
  seat,
  index,
  seatCount,
  winner,
  onChooseWinner,
  onMove,
  onRemove,
  canRemove = true,
  children,
}: SeatEditorProps) {
  return (
    <article className="border-base-300 bg-base-100 rounded-box border p-4">
      <div className="mb-3 flex items-center gap-2">
        <span className="bg-neutral text-neutral-content grid size-7 place-items-center rounded-full text-xs font-bold">
          {index + 1}
        </span>
        <label className="flex flex-1 items-center gap-2 font-semibold">
          <input
            type="radio"
            name="winner"
            className="radio radio-success radio-sm"
            checked={winner}
            onChange={onChooseWinner}
            aria-label={`${seat.playerName || `Seat ${index + 1}`} won`}
          />
          <Trophy className="text-success size-4" /> Winner
        </label>
        <button
          type="button"
          className="btn btn-square btn-ghost btn-xs"
          disabled={index === 0}
          onClick={() => onMove(-1)}
          aria-label="Move seat up"
        >
          <ArrowUp className="size-4" />
        </button>
        <button
          type="button"
          className="btn btn-square btn-ghost btn-xs"
          disabled={index === seatCount - 1}
          onClick={() => onMove(1)}
          aria-label="Move seat down"
        >
          <ArrowDown className="size-4" />
        </button>
        <button
          type="button"
          className="btn btn-square btn-ghost btn-xs text-error"
          disabled={!canRemove || seatCount <= 2}
          onClick={onRemove}
          aria-label="Remove seat"
        >
          <Trash2 className="size-4" />
        </button>
      </div>
      {children}
    </article>
  )
}
