import { Check, Dices, SkipForward } from "lucide-react"
import { useState } from "react"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { useDeckChooser } from "@/features/decks/deck-chooser"

const EMPTY_MESSAGES = {
  player_not_linked: "Your account needs a linked player before a deck can be picked.",
  no_eligible_decks: "None of your decks are included in random picks.",
}

/** Rolls the deck chooser's weighted pick from the table. Playing it records the choice and seats
 * the deck, exactly like the /decks/choose page followed by picking the commander by hand. */
export function DeckChooserButton({ onChooseDeck }: { onChooseDeck: (deckId: number) => void }) {
  const [open, setOpen] = useState(false)
  const chooser = useDeckChooser({ enabled: open })
  const { pick, outcome } = chooser
  const candidate = pick.data?.deck ? pick.data : undefined

  async function play() {
    const deck = await chooser.play()
    if (!deck) return
    onChooseDeck(deck.id)
    chooser.reset()
    setOpen(false)
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="btn-square h-auto min-h-8"
          aria-label="Pick a deck for me"
          title="Pick a deck for me"
        >
          <Dices className="size-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="relative w-72 overflow-hidden p-0">
        <CardArtBackground
          imageUrl={candidate?.deck.commander_art_crop_url}
          partnerImageUrl={candidate?.deck.partner_art_crop_url}
        />
        <div className="relative grid gap-3 p-3" aria-live="polite">
          <div className="text-base-content/55 flex items-center gap-1.5 text-[0.65rem] font-bold tracking-wider uppercase">
            <Dices className="size-3" /> Deck chooser
          </div>
          {pick.isPending && <p className="text-base-content/65 text-sm">Rolling a deck…</p>}
          {pick.isError && (
            <p role="alert" className="text-error text-sm">
              Could not choose a deck. Try again.
            </p>
          )}
          {pick.data?.reason && (
            <p className="text-base-content/70 text-sm">{EMPTY_MESSAGES[pick.data.reason]}</p>
          )}
          {candidate && (
            <>
              <div className="min-w-0">
                <p className="text-base font-black leading-tight">
                  <DeckCommanders deck={candidate.deck} />
                </p>
                <p className="text-base-content/70 truncate text-xs">{candidate.deck.name}</p>
                <ColorIdentity colors={candidate.deck.color_identity} className="mt-1.5" />
                <p className="text-base-content/60 mt-1.5 text-[0.65rem]">
                  {candidate.play_count} {candidate.play_count === 1 ? "play" : "plays"} ·{" "}
                  {candidate.last_played_at
                    ? `last played ${formatDate(candidate.last_played_at)}`
                    : "never played"}
                </p>
              </div>
              {outcome.isError && (
                <p role="alert" className="text-error text-xs">
                  That choice could not be saved. Try again.
                </p>
              )}
              <div className="grid grid-cols-2 gap-1.5">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void chooser.skip()}
                  disabled={outcome.isPending}
                >
                  <SkipForward className="size-3.5" /> Skip
                </Button>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void play()}
                  disabled={outcome.isPending}
                >
                  <Check className="size-3.5" /> Play this
                </Button>
              </div>
            </>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(value))
}
