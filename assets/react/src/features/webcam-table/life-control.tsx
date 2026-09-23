import { Minus, Plus } from "lucide-react"
import { useState, type ReactNode } from "react"
import { cn } from "@/lib/cn"

/** The life box is the keyboard/touch entry point; camera selection stays separate. */
export function LifeControl({
  life,
  local,
  size,
  counters,
  onChangeLife,
}: {
  life: number
  local: boolean
  size: "board" | "tile"
  counters: (onOpenChange: (open: boolean) => void) => ReactNode
  onChangeLife: (delta: number) => void
}) {
  const [hovered, setHovered] = useState(false)
  const [focused, setFocused] = useState(false)
  const [countersOpen, setCountersOpen] = useState(false)
  const expanded = hovered || focused || countersOpen
  const box = cn(
    "grid min-w-14 place-items-center rounded-md border border-white/25 bg-black/80 px-2 font-black text-white tabular-nums shadow",
    size === "board" ? "h-16 text-4xl" : "h-14 text-3xl",
  )

  return (
    <div
      className="absolute top-1.5 left-1.5 flex items-start gap-1"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false)
      }}
    >
      <div className="grid gap-0.5">
        {local ? (
          <button type="button" className={box} aria-label={`${life} life; show life controls`}>
            {life}
          </button>
        ) : (
          <span className={box} aria-label={`${life} life`}>
            {life}
          </span>
        )}
        <div className={cn(local && !expanded && "invisible")}>{counters(setCountersOpen)}</div>
      </div>
      {local && (
        <div className="grid gap-0.5" hidden={!expanded}>
          <button
            type="button"
            className="btn btn-sm btn-square size-8 border-white/25 bg-base-100 text-base-content"
            aria-label="Gain 1 life"
            onClick={() => onChangeLife(1)}
          >
            <Plus className="size-4" />
          </button>
          <button
            type="button"
            className="btn btn-sm btn-square size-8 border-white/25 bg-base-100 text-base-content"
            aria-label="Lose 1 life"
            onClick={() => onChangeLife(-1)}
          >
            <Minus className="size-4" />
          </button>
        </div>
      )}
    </div>
  )
}
