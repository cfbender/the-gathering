import { X } from "lucide-react"
import type { DeckSummary } from "@/features/decks/decks"
import type { CapturedCard } from "./use-webcam-room"

interface Props {
  capture: CapturedCard
  playerName: string
  suggestions: DeckSummary[]
  onChoose: (deckId: number) => void
  onDismiss: () => void
}

/** Compact popover over the board after a card click: the native crop plus up to five
 * numbered deck suggestions (keys 1–5) for the clicked player's seat. */
export function CardSuggestions({ capture, playerName, suggestions, onChoose, onDismiss }: Props) {
  return (
    <section
      className="absolute bottom-4 left-1/2 z-10 w-[min(34rem,calc(100%-2rem))] -translate-x-1/2 rounded-xl border border-white/15 bg-black/85 text-white shadow-2xl backdrop-blur-xl"
      aria-label="Card suggestions"
    >
      <header className="flex items-center justify-between px-3 pt-2">
        <span className="text-[0.65rem] font-bold tracking-wider text-white/60 uppercase">
          Identify card · {playerName}’s commander
        </span>
        <button
          type="button"
          className="grid size-6 place-items-center rounded-full text-white/60 hover:bg-white/10 hover:text-white"
          onClick={onDismiss}
          aria-label="Dismiss suggestions"
        >
          <X className="size-3.5" />
        </button>
      </header>
      <div className="grid gap-3 p-3 sm:grid-cols-[5.5rem_1fr]">
        <img
          className="aspect-square w-full rounded-lg object-cover"
          src={capture.image}
          alt="Native camera crop around the clicked card"
        />
        <div className="min-w-0">
          <div className="grid gap-1">
            {suggestions.map((deck, index) => (
              <button
                key={deck.id}
                type="button"
                className="flex h-8 items-center gap-2 rounded-md border border-white/10 bg-white/5 px-2 text-left text-xs hover:bg-white/15"
                onClick={() => onChoose(deck.id)}
              >
                <kbd className="kbd kbd-xs bg-white text-black">{index + 1}</kbd>
                <span className="truncate font-semibold">{deck.commander_name}</span>
                <span className="ml-auto truncate text-white/50">{deck.name}</span>
              </button>
            ))}
            {suggestions.length === 0 && (
              <p className="text-xs text-white/65">No decks are recorded for {playerName} yet.</p>
            )}
          </div>
          <p className="mt-2 text-[0.65rem] text-white/45">
            {capture.nativeWidth}×{capture.nativeHeight} native crop · deck-based suggestions until
            recognition artifacts ship
          </p>
        </div>
      </div>
    </section>
  )
}
