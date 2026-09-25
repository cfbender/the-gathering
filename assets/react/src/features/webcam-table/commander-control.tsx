import { ChevronDown } from "lucide-react"
import { ColorIdentity } from "@/components/mana-symbols"
import type { DeckSummary } from "@/features/decks/decks"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { cn } from "@/lib/cn"
import { CommanderHover } from "@/components/card-hover"
import { CommanderPicker } from "./commander-picker"
import { commanderNames, counterValue } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

const COLOR_TEXT: Record<string, string> = {
  W: "text-amber-100",
  U: "text-sky-300",
  B: "text-violet-300",
  R: "text-red-300",
  G: "text-emerald-300",
  C: "text-slate-300",
  "": "text-white/85",
}

/** Current tax per commander, in name order; adjust it from the counters panel or hotkeys. */
function TaxBadge({ participant, names }: { participant: TableParticipant; names: string[] }) {
  const taxes = names.map(
    (commander) => 2 * counterValue(participant, { kind: "casts", commander }),
  )
  const label = `Commander tax: ${names.map((name, index) => `${name} +${taxes[index]}`).join(", ")}`
  return (
    <span
      className={cn(
        "shrink-0 rounded bg-black/45 px-1 font-bold text-white tabular-nums",
        taxes.every((tax) => tax === 0) && "text-white/55",
      )}
      role="img"
      aria-label={label}
      title={label}
    >
      {taxes.map((tax) => `+${tax}`).join("/")}
    </span>
  )
}

/** The compact name opens the owner's deck picker; the tax badge sits left of the name. */
export function CommanderControl({
  participant,
  decks,
  local,
  compact = false,
  onChooseDeck,
}: {
  participant: TableParticipant
  decks: DeckSummary[]
  local: boolean
  compact?: boolean
  onChooseDeck: (deckId: number) => void
}) {
  const deck = decks.find((candidate) => candidate.id === participant.deck_id)
  const names = commanderNames(deck)
  const label = names.join(" / ")

  return (
    <div className="flex min-w-0 items-center gap-1.5">
      {local ? (
        <CommanderPicker
          playerId={participant.player_id}
          playerName={participant.player_name}
          decks={decks}
          selectedDeckId={participant.deck_id}
          onChoose={onChooseDeck}
        >
          <button
            type="button"
            className={cn(
              "flex min-w-0 items-center gap-1 rounded px-1 py-1 text-xs font-semibold hover:bg-white/10",
              deck ? (COLOR_TEXT[deck.color_identity] ?? "text-amber-300") : "text-primary",
            )}
            title={label || "Select commander"}
            aria-label={`Choose ${participant.player_name}'s commander`}
          >
            {deck && <TaxBadge participant={participant} names={names} />}
            <CommanderHover deck={deck}>
              {deck ? <DeckCommanders deck={deck} compact /> : <span>Select commander</span>}
            </CommanderHover>
            {deck && !compact && <ColorIdentity colors={deck.color_identity} />}
            <ChevronDown className="size-3 shrink-0 opacity-70" />
          </button>
        </CommanderPicker>
      ) : (
        // Only a seat's owner picks its commander; other seats just see it.
        <span
          className={cn(
            "flex min-w-0 items-center gap-1 px-1 py-1 text-xs font-semibold",
            deck ? (COLOR_TEXT[deck.color_identity] ?? "text-amber-300") : "text-white/45",
          )}
          title={label || undefined}
        >
          {deck && <TaxBadge participant={participant} names={names} />}
          <CommanderHover deck={deck}>
            {deck ? <DeckCommanders deck={deck} compact /> : <span>No commander yet</span>}
          </CommanderHover>
          {deck && !compact && <ColorIdentity colors={deck.color_identity} />}
        </span>
      )}
    </div>
  )
}
