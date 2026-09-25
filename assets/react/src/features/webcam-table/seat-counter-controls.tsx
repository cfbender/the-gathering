import { ChevronDown, Crown, Minus, Plus, ShieldAlert } from "lucide-react"
import { useEffect, useState } from "react"
import { CardImage } from "@/components/card-image"
import { GameChangerBadge } from "@/components/game-changer-badge"
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
  onChangeLife: (delta: number) => void
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
  visibleLabel = label,
  onChangeLife,
  gameChanger,
}: {
  label: string
  value: number
  local: boolean
  onAdjust: (delta: number) => void
  threshold?: number
  multiplier?: number
  art?: string | null
  visibleLabel?: string
  onChangeLife?: (delta: number) => void
  gameChanger?: boolean
}) {
  // Net damage added since the last apply; removing damage never offers to gain life.
  const [pending, setPending] = useState(0)
  // Bumped on every click so the offer only appears once clicking pauses.
  const [clicks, setClicks] = useState(0)
  const [settled, setSettled] = useState(false)
  useEffect(() => {
    if (!clicks) return
    const timeout = window.setTimeout(() => setSettled(true), 500)
    return () => window.clearTimeout(timeout)
  }, [clicks])
  useEffect(() => {
    if (!settled || !pending) return
    const timeout = window.setTimeout(() => setPending(0), 5000)
    return () => window.clearTimeout(timeout)
  }, [pending, settled])
  const adjust = (delta: number) => {
    onAdjust(delta)
    if (!onChangeLife) return
    setPending((previous) => Math.max(0, previous + delta))
    setSettled(false)
    setClicks((previous) => previous + 1)
  }
  const warning = threshold !== undefined && value >= threshold
  return (
    <div
      className={cn("flex flex-wrap items-center gap-2 py-1", warning && "text-error")}
      title={label}
    >
      {art !== undefined && (
        <CardImage
          imageUris={{ art_crop: art ?? undefined }}
          name={label}
          className="size-8 shrink-0"
        />
      )}
      <span className="min-w-0 flex-1 text-xs">{visibleLabel}</span>
      <GameChangerBadge gameChanger={gameChanger} compact />
      {local && (
        <button
          type="button"
          className="btn btn-ghost btn-sm btn-square shrink-0"
          aria-label={`Decrease ${label}`}
          disabled={value === 0}
          onClick={() => adjust(-1)}
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
          onClick={() => adjust(1)}
        >
          {multiplier === 1 ? <Plus className="size-4" /> : `+${multiplier}`}
        </button>
      )}
      {warning && (
        <ShieldAlert className="size-4 shrink-0" aria-label={`${label} lethal threshold reached`} />
      )}
      {local && settled && pending > 0 && onChangeLife && (
        <div className="flex w-full justify-end">
          <button
            type="button"
            className="btn btn-xs btn-soft"
            aria-label={`Apply -${pending} life for ${label}`}
            onClick={() => {
              onChangeLife(-pending)
              setPending(0)
            }}
          >
            Also −{pending} life
          </button>
        </div>
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
  onChangeLife,
  onTakeMonarch,
  onOpenChange,
}: Props) {
  const deck = decks.find((candidate) => candidate.id === participant.deck_id)
  const opponents = participants.filter((seat) => seat.player_id !== participant.player_id)
  const sourceIds = new Set([
    ...opponents.map((seat) => String(seat.player_id)),
    ...Object.keys(participant.commander_damage),
  ])
  const damageRows = [...sourceIds].flatMap((id) => {
    const source = opponents.find((seat) => String(seat.player_id) === id)
    const sourceDeck = decks.find((deck) => deck.id === source?.deck_id)
    const names = new Set([
      ...commanderNames(sourceDeck),
      ...Object.keys(participant.commander_damage[id] ?? {}),
    ])
    return [...names].map((commander) => ({
      playerId: Number(id),
      commander,
      playerName: source?.player_name ?? `Player ${id} (left)`,
      art:
        commander === sourceDeck?.commander_name
          ? sourceDeck.commander_art_crop_url
          : commander === sourceDeck?.partner_name
            ? sourceDeck.partner_art_crop_url
            : null,
    }))
  })
  const row = (
    label: string,
    counter: Counter,
    options: {
      threshold?: number
      multiplier?: number
      art?: string | null
      gameChanger?: boolean
    } = {},
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
              gameChanger:
                commander === deck?.commander_name
                  ? deck?.commander_game_changer
                  : deck?.partner_game_changer,
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
            21 from one commander is lethal. After adjusting, optionally apply the change to life.
          </p>
          {damageRows.map(({ playerId, commander, playerName, art }) => (
            <CounterRow
              key={`${playerId}:${commander}`}
              label={`${playerName} · ${commander}`}
              visibleLabel={art ? commander : `${playerName} · ${commander}`}
              art={art}
              value={counterValue(participant, { kind: "damage", playerId, commander })}
              local={local}
              threshold={21}
              onAdjust={(delta) => onAdjust({ kind: "damage", playerId, commander }, delta)}
              onChangeLife={onChangeLife}
            />
          ))}
          {opponents
            .filter((seat) => !damageRows.some((row) => row.playerId === seat.player_id))
            .map((seat) => (
              <div
                key={seat.player_id}
                className="flex items-center gap-2 py-2 text-xs text-base-content/55"
              >
                <CardImage imageUris={{}} name={seat.player_name} className="size-8 shrink-0" />
                <span>{seat.player_name} · Commander not revealed</span>
              </div>
            ))}
          {damageRows.length === 0 && opponents.length === 0 && (
            <p className="py-2 text-xs text-base-content/55">
              Opponents must select decks to track damage.
            </p>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
