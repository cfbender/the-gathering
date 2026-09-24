import { Minus, Plus } from "lucide-react"
import { useRef, useState, type ReactNode } from "react"
import { cn } from "@/lib/cn"

/**
 * The owner types a new total directly; it commits on Enter or blur as one signed delta, so
 * seat and Two-Headed Giant team life share the same `onChangeLife` path. Escape discards.
 */
function LifeInput({
  life,
  className,
  onChangeLife,
}: {
  life: number
  className: string
  onChangeLife: (delta: number) => void
}) {
  const [draft, setDraft] = useState<string | null>(null)
  const discard = useRef(false)
  const value = draft ?? String(life)

  const commit = () => {
    const typed = draft?.trim() ?? ""
    if (!discard.current && /^-?\d+$/.test(typed)) {
      const delta = Math.max(-999, Math.min(999, Number(typed))) - life
      if (delta !== 0) onChangeLife(delta)
    }
    discard.current = false
    setDraft(null)
  }

  return (
    <input
      type="text"
      inputMode="numeric"
      autoComplete="off"
      maxLength={4}
      className={cn(className, "cursor-text text-center outline-none focus:border-primary")}
      style={{ width: `calc(${Math.max(value.length, 2)}ch + 1.25rem)` }}
      value={value}
      aria-label={`${life} life; edit life total`}
      title="Type a new life total"
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setDraft(event.currentTarget.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur()
        if (event.key === "Escape") {
          discard.current = true
          event.currentTarget.blur()
        }
      }}
    />
  )
}

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
          <LifeInput life={life} className={box} onChangeLife={onChangeLife} />
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
