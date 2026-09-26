import { Check, Dices, Library, SkipForward } from "lucide-react"
import { useState } from "react"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { Button } from "@/components/ui/button"
import { useDeckChooser, type DeckPick } from "@/features/decks/deck-chooser"
import { DeckCommanders } from "./deck-commanders"
import {
  SyncRemoteDecksButton,
  SyncRemoteDecksResult,
  useSyncRemoteDecks,
} from "@/features/decks/sync-remote-decks"

export function DeckChooserPage() {
  const [chosenName, setChosenName] = useState<string>()
  const chooser = useDeckChooser()
  const { pick, outcome } = chooser
  const sync = useSyncRemoteDecks(() => {
    chooser.reset()
    setChosenName(undefined)
  })

  async function skip() {
    setChosenName(undefined)
    await chooser.skip()
  }

  async function choose() {
    const deck = await chooser.play()
    if (deck) setChosenName(deck.name)
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="What should I play?"
        title="Choose a deck"
        description="A weighted pick favors decks you have not played recently, while your skips keep nudging a deck back into the mix."
        actions={<SyncRemoteDecksButton sync={sync} />}
      />

      <a href="/decks" className="link link-hover text-base-content/65 w-fit text-sm">
        ← Back to decks
      </a>

      <SyncRemoteDecksResult sync={sync} />
      {outcome.isError && (
        <div role="alert" className="alert alert-error">
          That choice could not be saved. Try again.
        </div>
      )}
      {chosenName && (
        <div role="status" className="alert alert-success">
          <Check className="size-5" />
          <span>
            <strong>{chosenName}</strong> is your pick. Have a great game!
          </span>
          <a href="/games/new" className="btn btn-sm">
            Log the game
          </a>
        </div>
      )}

      {pick.isPending && <ChooserSkeleton />}
      {pick.isError && (
        <div role="alert" className="alert alert-error">
          Could not choose a deck. Try again.
        </div>
      )}
      {pick.data?.reason === "player_not_linked" && (
        <EmptyPanel
          icon={<Library className="size-10" />}
          title="Link your player first"
          description="Your account needs a linked player before The Gathering can choose from your decks. Ask an administrator to link it."
        />
      )}
      {pick.data?.reason === "no_eligible_decks" && (
        <EmptyPanel
          icon={<Library className="size-10" />}
          title="No decks available to pick"
          description="Add a deck to your linked player, or include one of your existing decks in random picks."
        />
      )}
      {pick.data?.deck && (
        <DeckCandidate
          pick={pick.data}
          pending={outcome.isPending || chosenName !== undefined}
          chosen={chosenName !== undefined}
          onSkip={() => void skip()}
          onChoose={() => void choose()}
        />
      )}
    </div>
  )
}

function DeckCandidate({
  pick,
  pending,
  chosen,
  onSkip,
  onChoose,
}: {
  pick: Extract<DeckPick, { deck: object }>
  pending: boolean
  chosen: boolean
  onSkip: () => void
  onChoose: () => void
}) {
  const deck = pick.deck
  if (!deck) return null

  return (
    <section
      aria-live="polite"
      className="card border-base-300 bg-base-200 relative min-h-96 overflow-hidden border shadow-xl"
    >
      <CardArtBackground
        imageUrl={deck.commander_art_crop_url}
        partnerImageUrl={deck.partner_art_crop_url}
      />
      <div className="card-body text-base-content relative z-10 justify-end gap-5 p-6 sm:min-h-[32rem] sm:p-10">
        <div className="max-w-2xl">
          <div className="badge badge-primary badge-outline mb-3">Your pick</div>
          <h2 className="text-4xl font-black tracking-normal sm:text-6xl">{deck.name}</h2>
          <p className="text-base-content/85 mt-3 text-lg sm:text-xl">
            <DeckCommanders deck={deck} />
          </p>
          <ColorIdentity colors={deck.color_identity} className="mt-3 text-xl" />
          <div className="mt-5 flex flex-wrap gap-2 text-sm">
            <span className="badge badge-lg bg-base-100/75 border-base-300">
              {pick.play_count} {pick.play_count === 1 ? "play" : "plays"}
            </span>
            <span className="badge badge-lg bg-base-100/75 border-base-300">
              {pick.skip_count} current {pick.skip_count === 1 ? "skip" : "skips"}
            </span>
            <span className="badge badge-lg bg-base-100/75 border-base-300">
              {pick.last_played_at
                ? `Last played ${formatDate(pick.last_played_at)}`
                : "Never played"}
            </span>
          </div>
        </div>
        <div className="card-actions flex-col justify-end sm:flex-row">
          <Button
            className="w-full sm:w-auto"
            variant="outline"
            onClick={onSkip}
            disabled={pending}
          >
            <SkipForward className="size-4" /> Skip
          </Button>
          <Button className="w-full sm:w-auto" onClick={onChoose} disabled={pending}>
            <Check className="size-4" />
            {chosen ? "Deck chosen" : pending ? "Saving…" : "Play this deck"}
          </Button>
        </div>
      </div>
    </section>
  )
}

function ChooserSkeleton() {
  return (
    <div className="card border-base-300 bg-base-200 min-h-96 animate-pulse border sm:min-h-[32rem]">
      <div className="card-body justify-end">
        <Dices className="text-primary size-12" />
        <div className="bg-base-300 h-12 w-3/4 rounded" />
        <div className="bg-base-300 h-5 w-1/2 rounded" />
      </div>
    </div>
  )
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(value))
}
