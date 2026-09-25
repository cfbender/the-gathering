import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { api } from "@/lib/api"
import type { DeckSummary } from "@/features/decks/decks"

export type DeckPick =
  | {
      deck: DeckSummary
      play_count: number
      skip_count: number
      last_played_at: string | null
      reason: null
    }
  | {
      deck: null
      reason: "player_not_linked" | "no_eligible_decks"
    }

export function getDeckPick(excludeId?: number) {
  const query = excludeId ? `?exclude_id=${excludeId}` : ""
  return api<{ data: DeckPick }>(`/api/deck-chooser${query}`).then((body) => body.data)
}

export function recordDeckOutcome(deckId: number, outcome: "played" | "skipped") {
  return api<{ data: { deck_id: number; outcome: string; skip_count: number } }>(
    `/api/deck-chooser/${deckId}/outcomes`,
    { method: "POST", body: JSON.stringify({ outcome }) },
  ).then((body) => body.data)
}

/** The weighted pick for the signed-in player's linked decks. Skipping records the skip, then asks
 * for a different candidate; playing records the choice and returns the deck. */
export function useDeckChooser({ enabled = true }: { enabled?: boolean } = {}) {
  const queryClient = useQueryClient()
  const [excludeId, setExcludeId] = useState<number>()
  const pick = useQuery({
    queryKey: ["deck-chooser", excludeId ?? null],
    queryFn: () => getDeckPick(excludeId),
    enabled,
  })
  const outcome = useMutation({
    mutationFn: ({ deckId, outcome }: { deckId: number; outcome: "played" | "skipped" }) =>
      recordDeckOutcome(deckId, outcome),
  })

  async function skip() {
    const deck = pick.data?.deck
    if (!deck) return
    await outcome.mutateAsync({ deckId: deck.id, outcome: "skipped" })
    setExcludeId(deck.id)
  }

  async function play() {
    const deck = pick.data?.deck
    if (!deck) return undefined
    await outcome.mutateAsync({ deckId: deck.id, outcome: "played" })
    // Playing resets the deck's skips, so the next pick should be rolled fresh when it is shown.
    void queryClient.invalidateQueries({ queryKey: ["deck-chooser"], refetchType: "none" })
    return deck
  }

  return { pick, outcome, skip, play, reset: () => setExcludeId(undefined) }
}
