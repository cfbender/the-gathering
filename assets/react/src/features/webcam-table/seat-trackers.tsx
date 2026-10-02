import { Calculator, EyeOff, Minus, Plus } from "lucide-react"
import { useMemo, useState } from "react"
import { cn } from "@/lib/cn"
import {
  COUNTER_MAX,
  combatRows,
  formatBuff,
  type CombatRow,
  type CustomCounter,
  type SeatTrackers,
} from "./trackers"
import { TrackersDialog } from "./trackers-dialog"

const PILL =
  "pointer-events-auto flex max-w-full items-center gap-1 rounded border border-white/20 bg-black/80 text-white shadow"

function PrivateMark({ what, compact }: { what: string; compact: boolean }) {
  return (
    <EyeOff
      className={cn("shrink-0 text-white/55", compact ? "size-2.5" : "size-3")}
      aria-label={`${what} is only visible to you`}
    />
  )
}

function CounterPill({
  counter,
  compact,
  onAdjust,
}: {
  counter: CustomCounter
  compact: boolean
  /** Only the seat's owner adjusts; others see the value. */
  onAdjust?: (delta: number) => void
}) {
  return (
    <div
      className={cn(PILL, compact ? "h-5 px-1.5 text-[0.6rem]" : "h-7 px-2 text-xs")}
      title={counter.label}
    >
      {!counter.shared && <PrivateMark what={counter.label} compact={compact} />}
      <span className="max-w-32 truncate whitespace-nowrap">{counter.label}</span>
      {onAdjust && !compact && (
        <button
          type="button"
          className="grid size-5 place-items-center rounded hover:bg-white/20 disabled:opacity-30"
          aria-label={`Decrease ${counter.label}`}
          disabled={counter.value === 0}
          onClick={() => onAdjust(-1)}
        >
          <Minus className="size-3" />
        </button>
      )}
      <span
        className={cn("min-w-[2ch] text-center font-black tabular-nums", !compact && "text-sm")}
        aria-label={`${counter.label}: ${counter.value}`}
      >
        {counter.value}
      </span>
      {onAdjust && !compact && (
        <button
          type="button"
          className="grid size-5 place-items-center rounded hover:bg-white/20 disabled:opacity-30"
          aria-label={`Increase ${counter.label}`}
          disabled={counter.value === COUNTER_MAX}
          onClick={() => onAdjust(1)}
        >
          <Plus className="size-3" />
        </button>
      )}
    </div>
  )
}

function CombatSummary({
  rows,
  shared,
  compact,
}: {
  rows: CombatRow[]
  shared: boolean
  compact: boolean
}) {
  return (
    <ul
      className={cn(PILL, "flex-col items-stretch gap-0", compact ? "px-1.5 py-0.5" : "px-2 py-1")}
      aria-label="Combat buffs"
    >
      {rows.map((row, index) => {
        const keywords = row.keywords.join(", ")
        return (
          <li
            key={row.key}
            className={cn(
              "flex items-baseline justify-between gap-2",
              compact ? "text-[0.6rem]" : "text-xs",
            )}
            title={keywords ? `${row.label} · ${keywords}` : row.label}
          >
            <span className="flex min-w-0 items-center gap-1 whitespace-nowrap">
              {index === 0 && !shared && <PrivateMark what="Combat buffs" compact={compact} />}
              <span className="min-w-0 truncate">
                {row.label}
                {keywords && <span className="text-white/60"> · {keywords}</span>}
              </span>
            </span>
            <strong className={cn("shrink-0 tabular-nums", !compact && "text-sm")}>
              {formatBuff(row.power, row.toughness)}
            </strong>
          </li>
        )
      })}
    </ul>
  )
}

/**
 * A seat's custom counters and combined combat buffs over its video. The owner sees every
 * tracker (private ones marked) with ± on their counters and a button opening the editor;
 * everyone else sees what the owner shares.
 */
export function SeatTrackersOverlay({
  trackers,
  playerName,
  compact = false,
  local,
}: {
  trackers: SeatTrackers
  playerName: string
  /** Rail tiles: smaller text, no ± buttons. */
  compact?: boolean
  /** Present for the seat's owner: makes the overlay editable. */
  local?: {
    adjustCounter: (id: string, delta: number) => void
    setTrackers: (update: (current: SeatTrackers) => SeatTrackers) => void
  }
}) {
  const [editing, setEditing] = useState(false)
  const rows = useMemo(() => combatRows(trackers.effects), [trackers.effects])
  if (!local && trackers.counters.length === 0 && rows.length === 0) return null
  return (
    <div
      className={cn(
        "pointer-events-none flex flex-col items-end",
        "max-w-full",
        compact ? "gap-0.5" : "gap-1",
      )}
      aria-label={`${playerName}'s trackers`}
    >
      {local && (
        <button
          type="button"
          className={cn(
            PILL,
            "hover:bg-primary/80",
            compact ? "h-5 px-1.5 text-[0.6rem]" : "h-7 px-2 text-xs",
          )}
          title="Custom counters and combat math"
          onClick={() => setEditing(true)}
        >
          <Calculator className={compact ? "size-3" : "size-3.5"} />
          Trackers
        </button>
      )}
      {trackers.counters.map((counter) => (
        <CounterPill
          key={counter.id}
          counter={counter}
          compact={compact}
          onAdjust={local ? (delta) => local.adjustCounter(counter.id, delta) : undefined}
        />
      ))}
      {rows.length > 0 && (
        <CombatSummary rows={rows} shared={trackers.shareEffects} compact={compact} />
      )}
      {local && (
        <TrackersDialog
          open={editing}
          onOpenChange={setEditing}
          trackers={trackers}
          onChange={local.setTrackers}
        />
      )}
    </div>
  )
}
