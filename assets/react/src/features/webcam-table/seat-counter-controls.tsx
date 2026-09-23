import { ChevronDown, Crown, Minus, Plus, ShieldAlert } from "lucide-react"
import { CardImage } from "@/components/card-image"
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
  onOpenChange?: (open: boolean) => void
}

function CounterRow({
  label,
  value,
  local,
  onAdjust,
  threshold,
  multiplier = 1,
  art,
}: {
  label: string
  value: number
  local: boolean
  onAdjust: (delta: number) => void
  threshold?: number
  multiplier?: number
  art?: string | null
}) {
  const warning = threshold !== undefined && value >= threshold
  return (
    <div className={cn("flex items-center gap-2 py-1", warning && "text-error")}>
      {art !== undefined && (
        <CardImage
          imageUris={{ art_crop: art ?? undefined }}
          name={label}
          className="size-8 shrink-0"
        />
      )}
      <span className="min-w-0 flex-1 text-xs">{label}</span>
      {local && (
        <button
          type="button"
          className="btn btn-ghost btn-sm btn-square shrink-0"
          aria-label={`Decrease ${label}`}
          disabled={value === 0}
          onClick={() => onAdjust(-1)}
        >
          {multiplier === 1 ? <Minus className="size-4" /> : `−${multiplier}`}
        </button>
      )}
      <span
        className="min-w-8 text-center text-sm font-bold tabular-nums"
        aria-label={`${label}: ${value * multiplier}`}
      >
        {value * multiplier}
      </span>
      {local && (
        <button
          type="button"
          className="btn btn-ghost btn-sm btn-square shrink-0"
          aria-label={`Increase ${label}`}
          disabled={value === 999}
          onClick={() => onAdjust(1)}
        >
          {multiplier === 1 ? <Plus className="size-4" /> : `+${multiplier}`}
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
  onOpenChange,
}: Props) {
  const deck = decks.find((candidate) => candidate.id === participant.deck_id)
  const namesFor = (seat: TableParticipant) =>
    commanderNames(decks.find((deck) => deck.id === seat.deck_id))
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
    options: { threshold?: number; multiplier?: number; art?: string | null } = {},
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
    <Popover onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            "flex h-6 w-full items-center justify-center rounded border border-white/20 bg-black/80 text-white/85 hover:bg-primary/80 data-[state=open]:bg-primary/80",
            counterWarning(participant) && "text-error",
          )}
          aria-label={`${participant.player_name}'s counters`}
          title="Counters & commander damage"
        >
          <ChevronDown className="size-4" />
        </button>
      </PopoverTrigger>
      <PopoverContent
        side="bottom"
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
        {commanderNames(deck).map((commander) =>
          row(
            `${commander} commander tax`,
            { kind: "casts", commander },
            {
              multiplier: 2,
              art:
                (commander === deck?.commander_name
                  ? deck?.commander_art_crop_url
                  : deck?.partner_art_crop_url) ?? null,
            },
          ),
        )}
        {!deck && (
          <p className="mb-2 text-xs text-base-content/55">Select a commander to track tax.</p>
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
