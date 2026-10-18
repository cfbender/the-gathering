import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { api } from "@/lib/api"
import type { DeckSummary } from "@/features/decks/decks"

/** Why a deck chooser script was not used, with the script line Lua reported. */
export interface ScriptError {
  message: string
  line: number | null
}

export type DeckPick =
  | {
      deck: DeckSummary
      play_count: number
      skip_count: number
      last_played_at: string | null
      /** The deck's share of the total weight, 0 to 1. */
      chance?: number
      /** The script's explanation for the weight. */
      note?: string | null
      /** Set when the saved script failed and the pick used the built-in weights. */
      script_error?: ScriptError | null
      reason: null
    }
  | {
      deck: null
      reason: "player_not_linked" | "no_eligible_decks"
    }

/** The browser's IANA zone, which deck chooser scripts see as `ctx.now`. */
export function browserTimeZone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone
}

export function getDeckPick(excludeId?: number) {
  const params = new URLSearchParams()
  if (excludeId) params.set("exclude_id", String(excludeId))
  params.set("tz", browserTimeZone())
  return api<{ data: DeckPick }>(`/api/deck-chooser?${params}`).then((body) => body.data)
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

// Scripts

/** A Lua value as the script receives it, decoded from JSON (`null` is `nil`). */
export type ScriptValue =
  | null
  | boolean
  | number
  | string
  | ScriptValue[]
  | { [key: string]: ScriptValue }

export type ScriptTable = { [key: string]: ScriptValue }

/** One deck as the preview weighed it. */
export interface PreviewDeck {
  id: number
  name: string
  base_weight: number
  weight: number
  chance: number
  note: string | null
  /** The `deck` table the script received. */
  input: ScriptTable
}

export interface ScriptPreview {
  reason: "player_not_linked" | "no_eligible_decks" | null
  /** `null` without a linked player. */
  source: "script" | "built_in" | null
  error: ScriptError | null
  logs: string[]
  /** The `ctx` table without `decks`. */
  context: ScriptTable | null
  decks: PreviewDeck[]
}

export function getDeckScript() {
  return api<{ data: { script: string | null } }>("/api/deck-chooser/script").then(
    (body) => body.data.script,
  )
}

export function saveDeckScript(script: string | null) {
  return api<{ data: { script: string | null } }>("/api/deck-chooser/script", {
    method: "PUT",
    body: JSON.stringify({ script }),
  }).then((body) => body.data.script)
}

export function previewDeckScript(script: string | null) {
  return api<{ data: ScriptPreview }>("/api/deck-chooser/preview", {
    method: "POST",
    body: JSON.stringify({ script, tz: browserTimeZone() }),
  }).then((body) => body.data)
}
