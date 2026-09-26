import { Check, Layers } from "lucide-react"
import type { DeckSummary } from "@/features/decks/decks"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { cn } from "@/lib/cn"
import { CommanderHover } from "@/components/card-hover"
import { deckNames } from "./deck-hint"
import { DecklistButton } from "./decklist-dialog"
import { CommanderActions } from "./new-commander-dialog"
import { PanelSection } from "./panel-section"
import type { BoardCard, TableParticipant } from "./room-types"

export interface DecksTabProps {
  playerDecks: DeckSummary[]
  localParticipant: TableParticipant
  identifiedCards: BoardCard[]
  onChooseDeck: (deckId: number) => void
}

export function DecksTab({
  playerDecks,
  localParticipant: local,
  identifiedCards,
  onChooseDeck,
}: DecksTabProps) {
  const boardNames = deckNames({
    cards: identifiedCards
      .filter((entry) => entry.ownerPeerId === local.peer_id)
      .map((entry) => ({ name: entry.card.name })),
  })
  return (
    <PanelSection title="Your commanders" icon={Layers}>
      {playerDecks.length === 0 ? (
        <p className="text-base-content/65 text-xs">No decks are recorded for you yet.</p>
      ) : (
        <ul className="grid gap-1">
          {playerDecks.map((deck) => {
            const selected = deck.id === local.deck_id
            return (
              <li key={deck.id}>
                <CommanderHover deck={deck}>
                  <button
                    type="button"
                    className={cn(
                      "flex w-full items-center gap-2 rounded-field border px-2 py-1.5 text-left text-xs",
                      selected
                        ? "border-primary bg-primary/15"
                        : "border-white/10 hover:border-white/30",
                    )}
                    onClick={() => onChooseDeck(deck.id)}
                    aria-pressed={selected}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block font-semibold">
                        <DeckCommanders deck={deck} />
                      </span>
                      <span className="text-base-content/55 block truncate">{deck.name}</span>
                    </span>
                    {selected && <Check className="text-primary size-3.5" />}
                  </button>
                </CommanderHover>
              </li>
            )
          })}
        </ul>
      )}
      <DecklistButton
        deck={playerDecks.find((deck) => deck.id === local.deck_id)}
        boardNames={boardNames}
      />
      <CommanderActions
        playerId={local.player_id}
        deck={playerDecks.find((deck) => deck.id === local.deck_id)}
        onChoose={onChooseDeck}
      />
    </PanelSection>
  )
}
