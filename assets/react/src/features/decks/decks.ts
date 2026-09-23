import { api } from "@/lib/api"
import type { PlayerSummary, RecentGame } from "@/features/games/games"

export interface DeckSummary {
  id: number
  player_id: number
  name: string
  commander_card_id: string | null
  commander_name: string
  commander_printing_id?: string | null
  commander_art_crop_url: string | null
  commander_image_url?: string | null
  partner_card_id: string | null
  partner_name: string | null
  partner_printing_id?: string | null
  partner_art_crop_url: string | null
  partner_image_url?: string | null
  color_identity: string
  decklist_url: string | null
  decklist_source: string | null
  archived_at: string | null
  skip_count: number
  included_for_play: boolean
  player: PlayerSummary | null
}

export interface DeckDetail extends DeckSummary {
  games_played: number
  wins: number
  recent_games: RecentGame[]
}

export function commanderNames(deck: Pick<DeckSummary, "commander_name" | "partner_name">) {
  return [deck.commander_name, deck.partner_name].filter(Boolean).join(" / ")
}

/** Mirrors `Games.can_manage_deck?`: guest decks have no member owner. */
export function canManageDeck(viewer: { id: number; role: string } | undefined, deck: DeckSummary) {
  if (!viewer) return false
  return viewer.role === "admin" || deck.player?.user_id === viewer.id
}

export const getDecks = (playerId?: number) =>
  api<{ data: DeckSummary[] }>(`/api/decks${playerId ? `?player_id=${playerId}` : ""}`).then(
    (body) => body.data,
  )

export const getDeck = (id: string) =>
  api<{ data: DeckDetail }>(`/api/decks/${id}`).then((body) => body.data)

export const isRetired = (deck: Pick<DeckSummary, "archived_at">) => deck.archived_at !== null

/**
 * Retires a deck (or brings it back). A retired deck keeps its games and stats but is
 * hidden from the deck list, the game form's deck picker, and the front of the owner's
 * profile; `archived_at` is the existing column that carries this.
 */
export const setDeckRetired = (id: number, retired: boolean) =>
  api<{ data: DeckDetail }>(`/api/decks/${id}`, {
    method: "PATCH",
    body: JSON.stringify({ deck: { archived_at: retired ? new Date().toISOString() : null } }),
  }).then((body) => body.data)

/** Deletes a deck; its games move to `replacementDeckId` or, without one, keep no deck. */
export const deleteDeck = (id: number, replacementDeckId?: number) =>
  api<void>(
    `/api/decks/${id}${replacementDeckId ? `?replacement_deck_id=${replacementDeckId}` : ""}`,
    { method: "DELETE" },
  )
