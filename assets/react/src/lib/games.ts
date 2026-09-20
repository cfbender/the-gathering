import { api } from "@/lib/api"

export interface Player {
  id: number
  name: string
  avatar_url: string | null
  /** Account this player belongs to; linked by an administrator or Discord sign-in. */
  user_id: number | null
  archived_at: string | null
  games_played?: number
  wins?: number
  decks?: Deck[]
  recent_games?: RecentGame[]
}

export interface Deck {
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
  skip_count?: number
  included_for_play?: boolean
  player?: Player
  games_played?: number
  wins?: number
  recent_games?: RecentGame[]
}

export interface Seat {
  id: number
  player_id: number
  deck_id: number | null
  seat: number
  result: "win" | "loss" | "draw"
  mvp_card_id: string | null
  mvp_card_name: string | null
  mvp_art_crop_url: string | null
  notes: string | null
  player: Player
  deck: Deck | null
}

export interface Game {
  id: number
  played_at: string
  duration_minutes: number | null
  turns: number | null
  notes: string | null
  source: "manual" | "csv" | "mythic_track" | "discord"
  external_id: string | null
  created_by_user_id: number | null
  seats: Seat[]
}

export interface RecentGame {
  id: number
  played_at: string
  result: Seat["result"]
  deck: Deck | null
}

export interface Pagination {
  page: number
  per_page: number
  total: number
  total_pages: number
}

/**
 * Mirrors `Games.can_manage_player?`: admins manage everyone, members manage
 * their own linked player and unclaimed guests (players without an account).
 */
export function canManagePlayer(viewer: { id: number; role: string } | undefined, player: Player) {
  if (!viewer) return false
  return viewer.role === "admin" || player.user_id === null || player.user_id === viewer.id
}

/** Mirrors `Games.can_manage_deck?`: guest decks have no member owner. */
export function canManageDeck(viewer: { id: number; role: string } | undefined, deck: Deck) {
  if (!viewer) return false
  return viewer.role === "admin" || deck.player?.user_id === viewer.id
}

/** Mirrors `Games.can_manage_game?`: admins, creators, and seated linked players may edit. */
export function canManageGame(viewer: { id: number; role: string } | undefined, game: Game) {
  if (!viewer) return false
  return (
    viewer.role === "admin" ||
    game.created_by_user_id === viewer.id ||
    game.seats.some((seat) => seat.player.user_id === viewer.id)
  )
}

export const getPlayers = () => api<{ data: Player[] }>("/api/players").then((body) => body.data)
export const getPlayer = (id: string) =>
  api<{ data: Player }>(`/api/players/${id}`).then((body) => body.data)
/** Admin only: folds `sourceId` into `targetId` (seats, decks, identity) and deletes the source. */
export const mergePlayers = (sourceId: number, targetId: number) =>
  api<{ data: Player }>(`/api/players/${sourceId}/merge`, {
    method: "POST",
    body: JSON.stringify({ target_id: targetId }),
  }).then((body) => body.data)
/** Admin only: makes `playerId` the account's player, merging the account's current player into it. */
export const linkUserPlayer = (userId: number, playerId: number) =>
  api<{ data: Player }>(`/api/admin/users/${userId}/player`, {
    method: "PUT",
    body: JSON.stringify({ player_id: playerId }),
  }).then((body) => body.data)
export const getDecks = (playerId?: number) =>
  api<{ data: Deck[] }>(`/api/decks${playerId ? `?player_id=${playerId}` : ""}`).then(
    (body) => body.data,
  )
export const getDeck = (id: string) =>
  api<{ data: Deck }>(`/api/decks/${id}`).then((body) => body.data)
export const getGame = (id: string) =>
  api<{ data: Game }>(`/api/games/${id}`).then((body) => body.data)

export function getGames(params: Record<string, string | number | undefined>) {
  const query = new URLSearchParams()
  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined && value !== "") query.set(key, String(value))
  })
  return api<{ data: Game[]; pagination: Pagination }>(`/api/games?${query}`)
}

export function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(
    new Date(value),
  )
}
