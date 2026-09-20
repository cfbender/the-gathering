import { api } from "@/lib/api"
import type { PlayerSummary, RecentGame } from "@/features/games/games"

export interface DeckSummary {
  id: number
  player_id: number
  name: string
  commander_card_id: string | null
  commander_name: string
  commander_art_crop_url: string | null
  partner_card_id: string | null
  partner_name: string | null
  partner_art_crop_url: string | null
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
