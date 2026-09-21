import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Archive, ArchiveRestore } from "lucide-react"
import { ApiError } from "@/lib/api"
import { invalidateGameRelated } from "@/features/games/games"
import { isRetired, setDeckRetired, type DeckDetail } from "./decks"

/**
 * Retiring is the gentle alternative to deleting: the deck and its games stay on
 * record, but it drops out of the deck list, the game form's deck picker, and the
 * front of the owner's profile (where it moves under "Retired decks"). Reversible.
 */
export function RetireDeckCard({ deck }: { deck: DeckDetail }) {
  const queryClient = useQueryClient()
  const retired = isRetired(deck)
  const toggle = useMutation({
    mutationFn: () => setDeckRetired(deck.id, !retired),
    onSuccess: (saved) => {
      queryClient.setQueryData(["decks", String(deck.id)], saved)
      void invalidateGameRelated(queryClient)
    },
  })

  return (
    <section
      className="card border-base-300 bg-base-200 border"
      aria-labelledby="retire-deck-heading"
    >
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="retire-deck-heading" className="flex items-center gap-2 text-lg font-bold">
          {retired ? <ArchiveRestore className="size-5" /> : <Archive className="size-5" />}
          {retired ? "Retired deck" : "Retire deck"}
        </h2>
        <p className="text-base-content/70 text-sm">
          {retired
            ? `${deck.name} is retired: it stays out of the deck list and deck picker and sits under "Retired decks" on the profile. Its games and stats are unchanged.`
            : `Retiring ${deck.name} hides it from the deck list, the deck picker, and the front of the profile, where it moves under "Retired decks". Its games and stats stay on record, and you can bring it back any time.`}
        </p>
        <div>
          <button
            type="button"
            className="btn btn-outline"
            disabled={toggle.isPending}
            onClick={() => toggle.mutate()}
          >
            {retired ? (
              <>
                <ArchiveRestore className="size-4" /> Bring back
              </>
            ) : (
              <>
                <Archive className="size-4" /> Retire deck
              </>
            )}
          </button>
        </div>
        {toggle.isError && (
          <p role="alert" className="text-error text-sm">
            {(toggle.error instanceof ApiError && toggle.error.detail) ||
              `Could not ${retired ? "bring back" : "retire"} the deck.`}
          </p>
        )}
      </div>
    </section>
  )
}
