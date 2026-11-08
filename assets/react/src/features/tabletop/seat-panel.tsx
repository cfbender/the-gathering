import { Droplet, Flag, Minus, Plus, Skull, Swords, X } from "lucide-react"
import { useEffect, useRef, useState, type PointerEvent } from "react"
import { CommanderArt } from "@/components/commander-art"
import { cn } from "@/lib/cn"
import { seatTint } from "./board-grid"
import {
  commanderKey,
  eliminationReason,
  hasCommanders,
  lethalCommanderDamage,
  seatName,
  type TabletopGame,
} from "./tabletop"

const HOLD_DELAY_MS = 450
const HOLD_REPEAT_MS = 550
const HOLD_STEP = 10
/** How long a run of taps keeps adding to the same "+3" bubble. */
const DELTA_SETTLE_MS = 1600

const OUT_LABELS = {
  conceded: "Conceded",
  life: "Out of life",
  poison: "Poisoned",
  commander: "Commander damage",
} as const

/**
 * A tap changes the value by one; holding changes it by ten at once and keeps going. Taps from
 * a keyboard arrive as clicks without a pointer, so they count once too.
 */
function useHoldStep(onStep: (delta: number) => void, sign: 1 | -1) {
  const latest = useRef(onStep)
  latest.current = onStep
  const timers = useRef<{ delay?: number; repeat?: number; held: boolean }>({ held: false })

  const clear = () => {
    window.clearTimeout(timers.current.delay)
    window.clearInterval(timers.current.repeat)
  }
  useEffect(() => clear, [])

  return {
    onPointerDown: (event: PointerEvent<HTMLButtonElement>) => {
      if (event.button !== 0) return
      clear()
      timers.current.held = false
      timers.current.delay = window.setTimeout(() => {
        timers.current.held = true
        latest.current(sign * HOLD_STEP)
        timers.current.repeat = window.setInterval(
          () => latest.current(sign * HOLD_STEP),
          HOLD_REPEAT_MS,
        )
      }, HOLD_DELAY_MS)
    },
    onPointerUp: () => {
      const wasHeld = timers.current.held
      clear()
      if (!wasHeld && timers.current.delay !== undefined) latest.current(sign)
      timers.current.delay = undefined
    },
    onPointerLeave: () => {
      clear()
      timers.current.delay = undefined
    },
    onPointerCancel: () => {
      clear()
      timers.current.delay = undefined
    },
    onClick: (event: { detail: number }) => {
      if (event.detail === 0) latest.current(sign)
    },
  }
}

/** The running total of recent changes, shown beside the life total until taps settle. */
function useRecentDelta(value: number) {
  const [baseline, setBaseline] = useState(value)
  const delta = value - baseline
  useEffect(() => {
    if (delta === 0) return
    const timer = window.setTimeout(() => setBaseline(value), DELTA_SETTLE_MS)
    return () => window.clearTimeout(timer)
  }, [value, delta])
  return delta
}

export interface SeatActions {
  onLife: (delta: number) => void
  onPoison: (delta: number) => void
  onConcede: (conceded: boolean) => void
  onPassTurn: () => void
}

/** One player's corner of the board during play: their life total fills the panel, the left
 * half takes life away and the right half gives it back. */
export function SeatPanel({
  game,
  index,
  highlighted,
  actions,
  countersOpen,
  onCountersOpen,
}: {
  game: TabletopGame
  index: number
  /** Lit while the first player is being drawn. */
  highlighted: boolean
  actions: SeatActions
  /** Whether this seat's counters cover the panel; the board turns opponents into commander
   * damage targets meanwhile. */
  countersOpen: boolean
  onCountersOpen: (open: boolean) => void
}) {
  const seat = game.seats[index]!
  const name = seatName(seat, index)
  const delta = useRecentDelta(seat.life)
  const out = eliminationReason(seat, game.format)
  const active = game.activeSeat === index
  const minus = useHoldStep(actions.onLife, -1)
  const plus = useHoldStep(actions.onLife, 1)
  const commanderTotal = Object.values(seat.commanderDamage).reduce((sum, value) => sum + value, 0)

  return (
    <div
      className={cn(
        "rounded-box relative size-full overflow-hidden text-white transition-shadow duration-300",
        active && "ring-accent ring-4 ring-inset",
        highlighted && "ring-4 ring-white ring-inset",
      )}
      style={{ background: `color-mix(in oklch, ${seatTint(index)} 30%, black)` }}
    >
      <CommanderArt
        imageUrl={seat.deck?.artUrl}
        partnerImageUrl={seat.deck?.partnerArtUrl}
        imageClassName="opacity-55"
      />
      <div className="pointer-events-none absolute inset-0 bg-radial from-black/10 to-black/60" />

      {/* Two halves: less on the left, more on the right. */}
      <div className="absolute inset-0 grid grid-cols-2">
        <button
          type="button"
          className="group flex items-center justify-start pl-[5cqw] outline-none focus-visible:bg-white/10 active:bg-white/10"
          aria-label={`Lose life: ${name}`}
          {...minus}
        >
          <Minus className="size-[max(1rem,min(9cqh,9cqw))] opacity-45 group-active:opacity-90" />
        </button>
        <button
          type="button"
          className="group flex items-center justify-end pr-[5cqw] outline-none focus-visible:bg-white/10 active:bg-white/10"
          aria-label={`Gain life: ${name}`}
          {...plus}
        >
          <Plus className="size-[max(1rem,min(9cqh,9cqw))] opacity-45 group-active:opacity-90" />
        </button>
      </div>

      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
        {/* Recent changes sit above the total; an equal spacer below keeps the total centred. */}
        <span
          className={cn(
            "h-[12cqh] leading-none font-black tabular-nums drop-shadow-[0_1px_4px_rgb(0_0_0/0.8)] transition-opacity",
            delta === 0 ? "opacity-0" : "opacity-100",
            delta < 0 ? "text-error" : "text-success",
          )}
          style={{ fontSize: "min(12cqh, 10cqw)" }}
          aria-hidden="true"
        >
          {delta > 0 ? `+${delta}` : delta}
        </span>
        <span
          className="leading-none font-black tracking-tight tabular-nums drop-shadow-[0_2px_8px_rgb(0_0_0/0.6)]"
          style={{ fontSize: "min(42cqh, 30cqw)" }}
          aria-live="polite"
          aria-label={`${name}: ${seat.life} life`}
        >
          {seat.life}
        </span>
        <span className="h-[12cqh]" />
      </div>

      {/* The player's own edge: their name, counters, and the turn button sit nearest them,
          away from the middle of the device. */}
      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex flex-col items-center gap-[2cqh] p-[3cqh] pt-0">
        {active && !out && (
          <button
            type="button"
            className="btn btn-accent btn-sm pointer-events-auto rounded-full px-5 shadow-lg"
            onClick={actions.onPassTurn}
          >
            End turn
          </button>
        )}
        <div className="flex w-full items-end justify-between gap-2">
          <span
            className="min-w-0 truncate font-black drop-shadow-[0_1px_4px_rgb(0_0_0/0.8)]"
            style={{ fontSize: "max(0.85rem, min(7cqh, 5.5cqw))" }}
          >
            {name}
          </span>
          <button
            type="button"
            className="btn btn-sm pointer-events-auto shrink-0 gap-1.5 rounded-full border-white/15 bg-black/45 px-3 text-white hover:bg-black/65"
            onClick={() => onCountersOpen(true)}
            aria-label={`Counters: ${name}`}
          >
            {seat.poison > 0 && (
              <span className="flex items-center gap-0.5 tabular-nums">
                <Droplet className="text-success size-3.5" fill="currentColor" />
                {seat.poison}
              </span>
            )}
            {commanderTotal > 0 && (
              <span className="flex items-center gap-0.5 tabular-nums">
                <Swords className="text-warning size-3.5" />
                {commanderTotal}
              </span>
            )}
            {seat.poison === 0 && commanderTotal === 0 && <Swords className="size-4" />}
          </button>
        </div>
      </div>

      {out && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-start bg-black/55 p-[4cqh]">
          <span className="badge badge-lg gap-1.5 border-white/20 bg-black/70 font-bold text-white">
            {out === "conceded" ? <Flag className="size-4" /> : <Skull className="size-4" />}
            {OUT_LABELS[out]}
          </span>
        </div>
      )}

      {countersOpen && (
        <TrackersSheet
          game={game}
          index={index}
          actions={actions}
          onClose={() => onCountersOpen(false)}
        />
      )}
    </div>
  )
}

function Stepper({
  label,
  value,
  danger,
  onChange,
}: {
  label: string
  value: number
  danger: boolean
  onChange: (delta: number) => void
}) {
  const minus = useHoldStep(onChange, -1)
  const plus = useHoldStep(onChange, 1)
  return (
    <li className="bg-base-200 rounded-field flex items-center gap-2 py-1 pr-1 pl-3">
      <span className="min-w-0 flex-1 truncate text-sm font-bold">{label}</span>
      <button
        type="button"
        className="btn btn-sm btn-square btn-ghost"
        aria-label={`Less ${label}`}
        {...minus}
      >
        <Minus className="size-4" />
      </button>
      <span
        className={cn("w-8 text-center text-lg font-black tabular-nums", danger && "text-error")}
      >
        {value}
      </span>
      <button
        type="button"
        className="btn btn-sm btn-square btn-ghost"
        aria-label={`More ${label}`}
        {...plus}
      >
        <Plus className="size-4" />
      </button>
    </li>
  )
}

/** Poison and conceding, inside the seat so it faces its player. While it is open, commander
 * damage this seat takes is counted on the opponents' panels instead. */
function TrackersSheet({
  game,
  index,
  actions,
  onClose,
}: {
  game: TabletopGame
  index: number
  actions: SeatActions
  onClose: () => void
}) {
  const seat = game.seats[index]!
  const commanders = hasCommanders(game.format)

  return (
    <div className="bg-base-100/95 text-base-content absolute inset-0 z-10 flex flex-col backdrop-blur">
      <div className="border-base-300 flex items-center gap-2 border-b px-3 py-1.5">
        <h3 className="min-w-0 flex-1 truncate text-sm font-black">
          {seatName(seat, index)} · {seat.life} life
        </h3>
        <button
          type="button"
          className="btn btn-ghost btn-sm btn-square"
          aria-label="Close counters"
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-2">
        {commanders && (
          <p className="bg-warning/15 rounded-field mb-1.5 flex items-center gap-2 px-3 py-1.5 text-xs">
            <Swords className="text-warning size-4 shrink-0" />
            Tap an opponent's commander to add the damage it dealt you.
          </p>
        )}
        <ul className="grid gap-1.5">
          <Stepper
            label="Poison"
            value={seat.poison}
            danger={seat.poison >= 10}
            onChange={actions.onPoison}
          />
        </ul>
        <div className="mt-3 grid grid-cols-2 gap-2">
          <button
            type="button"
            className={cn("btn btn-sm", seat.conceded ? "btn-outline" : "btn-ghost text-error")}
            onClick={() => {
              actions.onConcede(!seat.conceded)
              onClose()
            }}
          >
            <Flag className="size-4" /> {seat.conceded ? "Back in" : "Concede"}
          </button>
          <button type="button" className="btn btn-sm btn-primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  )
}

/**
 * An opponent's panel while another seat's counters are open: one tap target per commander at
 * this seat, each counting the damage it has dealt to that seat. A tap adds one; the corner
 * button takes one back.
 */
export function CommanderDamagePanel({
  game,
  index,
  target,
  onChange,
}: {
  game: TabletopGame
  index: number
  /** The seat taking the damage. */
  target: number
  onChange: (key: string, delta: number) => void
}) {
  const seat = game.seats[index]!
  const targetSeat = game.seats[target]!
  const targetName = seatName(targetSeat, target)
  const lethal = lethalCommanderDamage(game.format)
  const commanders = [
    { key: commanderKey(index), card: seat.deck?.commanderName, art: seat.deck?.artUrl },
    ...(seat.deck?.partnerName
      ? [
          {
            key: commanderKey(index, true),
            card: seat.deck.partnerName,
            art: seat.deck.partnerArtUrl,
          },
        ]
      : []),
  ]

  return (
    <div
      className="rounded-box ring-warning relative grid size-full overflow-hidden text-white ring-4 ring-inset"
      style={{
        background: `color-mix(in oklch, ${seatTint(index)} 30%, black)`,
        gridTemplateColumns: `repeat(${commanders.length}, minmax(0, 1fr))`,
      }}
    >
      {commanders.map((commander, at) => {
        const damage = targetSeat.commanderDamage[commander.key] ?? 0
        const label = commander.card ?? seatName(seat, index)
        return (
          <div
            key={commander.key}
            className={cn("relative min-w-0", at > 0 && "border-l-2 border-black/60")}
          >
            <CommanderArt imageUrl={commander.art} imageClassName="opacity-35" />
            <button
              type="button"
              className="absolute inset-0 flex flex-col items-center justify-center gap-[2cqh] px-2 outline-none focus-visible:bg-white/10 active:bg-white/15"
              aria-label={`Commander damage from ${label} to ${targetName}: ${damage}. Add one`}
              onClick={() => onChange(commander.key, 1)}
            >
              <Swords
                className={cn(
                  "size-[max(1rem,10cqh)]",
                  damage >= lethal ? "text-error" : "text-warning",
                )}
              />
              <span
                className={cn(
                  "leading-none font-black tabular-nums drop-shadow-[0_2px_8px_rgb(0_0_0/0.6)]",
                  damage >= lethal && "text-error",
                )}
                style={{ fontSize: `min(36cqh, ${commanders.length > 1 ? 18 : 26}cqw)` }}
              >
                {damage}
              </span>
              <span
                className="max-w-full truncate font-bold drop-shadow-[0_1px_4px_rgb(0_0_0/0.8)]"
                style={{ fontSize: "max(0.75rem, min(6cqh, 4.5cqw))" }}
              >
                {label}
              </span>
            </button>
            <button
              type="button"
              className="btn btn-circle btn-sm absolute bottom-[3cqh] left-[3cqh] border-white/15 bg-black/55 text-white hover:bg-black/75"
              aria-label={`Less commander damage from ${label} to ${targetName}`}
              disabled={damage === 0}
              onClick={() => onChange(commander.key, -1)}
            >
              <Minus className="size-4" />
            </button>
          </div>
        )
      })}
      <p
        className="bg-warning text-warning-content pointer-events-none absolute top-[3cqh] left-1/2 max-w-[90%] -translate-x-1/2 truncate rounded-full px-3 py-0.5 font-bold shadow-lg"
        style={{ fontSize: "max(0.75rem, min(5cqh, 4cqw))" }}
      >
        Damage to {targetName}
      </p>
    </div>
  )
}
