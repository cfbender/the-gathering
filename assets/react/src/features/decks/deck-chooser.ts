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
