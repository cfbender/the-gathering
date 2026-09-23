import { ChevronDown } from "lucide-react"
import { ColorIdentity } from "@/components/mana-symbols"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import { CommanderPicker } from "./commander-picker"
import { commanderNames } from "./seat-counters"
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

/** The compact name opens the owner's deck picker; tax lives in the counters panel. */
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
  const label = commanderNames(deck).join(" / ")

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
            <CommanderHover deck={deck}>
              <span className="truncate">{label || "Select commander"}</span>
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
          <CommanderHover deck={deck}>
            <span className="truncate">{label || "No commander yet"}</span>
          </CommanderHover>
          {deck && !compact && <ColorIdentity colors={deck.color_identity} />}
        </span>
      )}
    </div>
  )
}
