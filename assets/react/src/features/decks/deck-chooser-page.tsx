import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Check, Dices, Library, RefreshCw, SkipForward } from "lucide-react"
import { useState } from "react"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { Button } from "@/components/ui/button"
import { useCurrentUser } from "@/lib/auth"
import {
  getDeckPick,
  recordDeckOutcome,
  syncManaVaultDecks,
  type DeckPick,
  type ManaVaultSyncResult,
} from "@/features/decks/deck-chooser"

export function DeckChooserPage() {
  const queryClient = useQueryClient()
  const user = useCurrentUser()
  const [excludeId, setExcludeId] = useState<number>()
  const [chosenName, setChosenName] = useState<string>()
  const pick = useQuery({
    queryKey: ["deck-chooser", excludeId ?? null],
    queryFn: () => getDeckPick(excludeId),
  })
  const outcome = useMutation({
    mutationFn: ({ deckId, outcome }: { deckId: number; outcome: "played" | "skipped" }) =>
      recordDeckOutcome(deckId, outcome),
  })
  const sync = useMutation({
    mutationFn: syncManaVaultDecks,
    onSuccess: () => {
      setExcludeId(undefined)
      setChosenName(undefined)
      void queryClient.invalidateQueries({ queryKey: ["deck-chooser"] })
      void queryClient.invalidateQueries({ queryKey: ["decks"] })
    },
  })
  const canSync = Boolean(user.data?.manavault_url && user.data.has_manavault_api_key)

  async function skip() {
    if (!pick.data?.deck) return
    setChosenName(undefined)
    await outcome.mutateAsync({ deckId: pick.data.deck.id, outcome: "skipped" })
    setExcludeId(pick.data.deck.id)
  }

  async function choose() {
    if (!pick.data?.deck) return
    await outcome.mutateAsync({ deckId: pick.data.deck.id, outcome: "played" })
    setChosenName(pick.data.deck.name)
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="What should I play?"
        title="Choose a deck"
        description="A weighted pick favors decks you have not played recently, while your skips keep nudging a deck back into the mix."
        actions={
          canSync ? (
            <Button variant="outline" onClick={() => sync.mutate()} disabled={sync.isPending}>
              <RefreshCw className={sync.isPending ? "size-4 animate-spin" : "size-4"} />
              {sync.isPending ? "Syncing…" : "Sync from ManaVault"}
            </Button>
          ) : undefined
        }
      />

      <a href="/decks" className="link link-hover text-base-content/65 w-fit text-sm">
        ← Back to decks
      </a>

      {sync.isSuccess && <SyncResult result={sync.data} />}
      {sync.isError && (
        <div role="alert" className="alert alert-error">
          ManaVault could not be synced. Check your instance URL and API key, then try again.
        </div>
      )}
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
      <CardArtBackground imageUrl={deck.commander_art_crop_url} />
      <div className="card-body text-base-content relative z-10 justify-end gap-5 p-6 sm:min-h-[32rem] sm:p-10">
        <div className="max-w-2xl">
          <div className="badge badge-primary badge-outline mb-3">Your pick</div>
          <h2 className="text-4xl font-black tracking-normal sm:text-6xl">{deck.name}</h2>
          <p className="text-base-content/85 mt-3 text-lg sm:text-xl">
            {deck.commander_name}
            {deck.partner_name && ` + ${deck.partner_name}`}
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

function SyncResult({ result }: { result: ManaVaultSyncResult }) {
  return (
    <div role="status" className="alert alert-success">
      <RefreshCw className="size-5" />
      Synced ManaVault: {result.created} created, {result.updated} updated.
    </div>
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
