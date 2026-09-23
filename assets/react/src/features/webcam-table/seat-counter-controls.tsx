import { Crown, Minus, Plus, ShieldAlert } from "lucide-react"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { commanderNames, counterValue, counterWarning, type Counter } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

interface Props {
  participant: TableParticipant
  participants: TableParticipant[]
  decks: DeckSummary[]
  local: boolean
  monarch: boolean
  onAdjust: (counter: Counter, delta: number) => void
  onTakeMonarch: () => void
}

function CounterRow({
  label,
  value,
  local,
  onAdjust,
  tax = false,
  threshold,
}: {
  label: string
  value: number
  local: boolean
  onAdjust: (delta: number) => void
  tax?: boolean
  threshold?: number
}) {
  const warning = threshold !== undefined && value >= threshold
  return (
    <div className={cn("flex items-center gap-2 py-1", warning && "text-error")}>
      <span className="min-w-0 flex-1 text-xs">{label}</span>
      {local && (
        <button
          type="button"
          className="btn btn-ghost btn-xs btn-square"
          aria-label={`Decrease ${label}`}
          disabled={value === 0}
          onClick={() => onAdjust(-1)}
        >
          <Minus className="size-3" />
        </button>
      )}
      <span
        className="min-w-8 text-center text-sm font-bold tabular-nums"
        aria-label={`${label}: ${tax ? value * 2 : value}${tax ? " tax" : ""}`}
      >
        {tax ? `+${value * 2}` : value}
      </span>
      {local && (
        <button
          type="button"
          className="btn btn-ghost btn-xs btn-square"
          aria-label={`Increase ${label}`}
          disabled={value === 999}
          onClick={() => onAdjust(1)}
        >
          <Plus className="size-3" />
        </button>
      )}
      {warning && (
        <ShieldAlert className="size-4 shrink-0" aria-label={`${label} lethal threshold reached`} />
      )}
    </div>
  )
}

/** Shared readout; only the seat's owner publishes counter changes, just like life. */
export function SeatCounterControls({
  participant,
  participants,
  decks,
  local,
  monarch,
  onAdjust,
  onTakeMonarch,
}: Props) {
  const namesFor = (seat: TableParticipant) =>
    commanderNames(decks.find((deck) => deck.id === seat.deck_id))
  const commanders = namesFor(participant)
  const opponents = participants.filter((seat) => seat.player_id !== participant.player_id)
  const sourceIds = new Set([
    ...opponents.map((seat) => String(seat.player_id)),
    ...Object.keys(participant.commander_damage),
  ])
  const damageRows = [...sourceIds].flatMap((id) => {
    const source = opponents.find((seat) => String(seat.player_id) === id)
    const names = new Set([
      ...(source ? namesFor(source) : []),
      ...Object.keys(participant.commander_damage[id] ?? {}),
    ])
    return [...names].map((commander) => ({
      playerId: Number(id),
      commander,
      playerName: source?.player_name ?? `Player ${id} (left)`,
    }))
  })
  const row = (
    label: string,
    counter: Counter,
    options: { tax?: boolean; threshold?: number } = {},
  ) => (
    <CounterRow
      key={label}
      label={label}
      value={counterValue(participant, counter)}
      local={local}
      onAdjust={(delta) => onAdjust(counter, delta)}
      {...options}
    />
  )

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            "flex h-6 shrink-0 items-center gap-1 rounded px-1 text-white/75 hover:bg-white/10",
            counterWarning(participant) && "text-error",
          )}
          aria-label={`${participant.player_name}'s counters`}
          title="Counters & commander damage"
        >
          <ShieldAlert className="size-3.5" />
          {participant.poison > 0 && <span className="text-[0.6rem]">P{participant.poison}</span>}
          {participant.rad > 0 && <span className="text-[0.6rem]">R{participant.rad}</span>}
        </button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="start"
        className="max-h-[70dvh] w-80 max-w-[calc(100vw-1.5rem)] overflow-y-auto p-3 text-base-content"
        aria-label={`${participant.player_name}'s counters`}
      >
        <div className="mb-2 flex items-center justify-between gap-2">
          <h2 className="text-sm font-bold">{participant.player_name}'s counters</h2>
          {monarch && (
            <span className="text-warning flex items-center gap-1 text-xs">
              <Crown className="size-4" /> Monarch
            </span>
          )}
        </div>
        {!local && (
          <p className="mb-2 text-xs text-base-content/55">This player manages their counters.</p>
        )}
        {row("Poison", { kind: "poison" }, { threshold: 10 })}
        {row("Rad", { kind: "rad" })}
        {local && (
          <button
            type="button"
            className="btn btn-sm btn-ghost my-2 w-full justify-start text-warning"
            disabled={monarch}
            onClick={onTakeMonarch}
          >
            <Crown className="size-4" />
            {monarch ? "You are the monarch" : "Take the monarch"}
          </button>
        )}
        <div className="mt-2 border-t border-base-content/10 pt-2">
          <h3 className="text-xs font-bold">Commander tax</h3>
          <p className="my-1 text-[0.65rem] text-base-content/55">
            +2 per cast from the command zone. Track each commander separately.
          </p>
          {commanders.map((commander) =>
            row(commander, { kind: "casts", commander }, { tax: true }),
          )}
          {commanders.length === 0 && (
            <p className="py-2 text-xs text-base-content/55">
              Select a deck to track commander tax.
            </p>
          )}
        </div>
        <div className="mt-2 border-t border-base-content/10 pt-2">
          <h3 className="text-xs font-bold">Commander damage received</h3>
          <p className="my-1 text-[0.65rem] text-base-content/55">
            21 from one commander is lethal. Adjust life separately.
          </p>
          {damageRows.map(({ playerId, commander, playerName }) =>
            row(
              `${playerName} · ${commander}`,
              { kind: "damage", playerId, commander },
              { threshold: 21 },
            ),
          )}
          {damageRows.length === 0 && (
            <p className="py-2 text-xs text-base-content/55">
              Opponents must select decks to track damage.
            </p>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
