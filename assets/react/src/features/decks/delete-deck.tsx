import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { Trash2 } from "lucide-react"
import { useState } from "react"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { ApiError } from "@/lib/api"
import { invalidateGameRelated } from "@/features/games/games"
import { deleteDeck, getDecks, type DeckDetail } from "./decks"

/**
 * Removes a deck. When it has been played, its games either move to another of the
 * player's decks (the fix for an accidental duplicate) or keep their seat with no deck.
 */
export function DeleteDeckCard({ deck }: { deck: DeckDetail }) {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const [replacementId, setReplacementId] = useState("")
  const [confirming, setConfirming] = useState(false)
  const others = useQuery({
    queryKey: ["decks", { player_id: deck.player_id }],
    queryFn: () => getDecks(deck.player_id),
    select: (decks) => decks.filter((candidate) => candidate.id !== deck.id),
    enabled: deck.games_played > 0,
  })
  const remove = useMutation({
    mutationFn: () => deleteDeck(deck.id, replacement?.id),
    onSuccess: () => {
      queryClient.removeQueries({ queryKey: ["decks", String(deck.id)] })
      void invalidateGameRelated(queryClient)
      if (deck.player) {
        void navigate({ to: "/players/$playerId", params: { playerId: String(deck.player.id) } })
      } else {
        void navigate({ to: "/decks", search: { scope: undefined } })
      }
    },
  })

  const played = deck.games_played > 0
  const replacement = others.data?.find((candidate) => String(candidate.id) === replacementId)
  const one = deck.games_played === 1
  const gamesLabel = `${deck.games_played} ${one ? "game" : "games"}`

  return (
    <section
      className="card border-base-300 bg-base-200 border"
      aria-labelledby="delete-deck-heading"
    >
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="delete-deck-heading" className="flex items-center gap-2 text-lg font-bold">
          <Trash2 className="size-5" /> Delete deck
        </h2>
        {played ? (
          <p className="text-base-content/70 text-sm">
            {deck.name} has {gamesLabel} on record. Move {one ? "it" : "them"} to another of{" "}
            {deck.player?.name ?? "this player"}'s decks, for example when this one is an accidental
            duplicate, or leave {one ? "that seat" : "those seats"} without a deck.
          </p>
        ) : (
          <p className="text-base-content/70 text-sm">
            {deck.name} has not been played, so nothing else changes.
          </p>
        )}
        <div className="flex flex-col gap-2 sm:flex-row">
          {played && (
            <select
              aria-label="Move games to"
              className="select min-w-0 flex-1"
              value={replacementId}
              onChange={(event) => setReplacementId(event.target.value)}
            >
              <option value="">Leave the {gamesLabel} without a deck</option>
              {others.data?.map((candidate) => (
                <option key={candidate.id} value={candidate.id}>
                  Move to {candidate.name}
                  {candidate.commander_name !== candidate.name && ` · ${candidate.commander_name}`}
                </option>
              ))}
            </select>
          )}
          <button
            type="button"
            className="btn btn-outline btn-error"
            disabled={remove.isPending}
            onClick={() => setConfirming(true)}
          >
            <Trash2 className="size-4" /> Delete deck
          </button>
        </div>
        {remove.isError && (
          <p role="alert" className="text-error text-sm">
            {(remove.error instanceof ApiError && remove.error.detail) ||
              "Could not delete the deck."}
          </p>
        )}
      </div>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Delete ${deck.name}?`}
        confirmLabel="Delete deck"
        destructive
        onConfirm={() => remove.mutate()}
      >
        {played && replacement && (
          <>
            Its {gamesLabel} {one ? "moves" : "move"} to <strong>{replacement.name}</strong>.{" "}
          </>
        )}
        {played && !replacement && (
          <>
            Its {gamesLabel} {one ? "stays" : "stay"} on record with <strong>no deck</strong>.{" "}
          </>
        )}
        <strong>{deck.name}</strong> is deleted. This cannot be undone.
      </ConfirmDialog>
    </section>
  )
}
