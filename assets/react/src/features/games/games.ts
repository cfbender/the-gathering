import type { QueryClient } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { DeckSummary } from "@/features/decks/decks"

export interface PlayerSummary {
  id: number
  name: string
  avatar_url: string | null
  /** Account this player belongs to; linked by an administrator or Discord sign-in. */
  user_id: number | null
  archived_at: string | null
}

export interface PlayerDetail extends PlayerSummary {
  discord_id: string | null
  games_played: number
  wins: number
  decks: DeckSummary[]
  recent_games: RecentGame[]
}

export interface Seat {
  id: number
  player_id: number
  deck_id: number | null
  seat: number
  result: "win" | "loss" | "draw"
  kills: number | null
  mvp_card_id: string | null
  mvp_card_name: string | null
  mvp_game_changer?: boolean
  mvp_art_crop_url: string | null
  notes: string | null
  player: PlayerSummary
  deck: DeckSummary | null
}

export interface Game {
  id: number
  played_at: string
  duration_minutes: number | null
  turns: number | null
  win_condition: WinCondition | null
  notes: string | null
  source: "manual" | "csv" | "mythic_track" | "discord"
  external_id: string | null
  created_by_user_id: number | null
  seats: Seat[]
}

export const WIN_CONDITIONS = [
  ["damage", "Damage"],
  ["infinite_combo", "Infinite Combo"],
  ["mill", "Mill"],
  ["poison", "Poison"],
  ["alternate_win_con", "On-card Alternate Win Con"],
  ["hard_lock", "Hard Lock"],
  ["commander_damage", "Commander Damage"],
  ["draw", "Draw"],
  ["non_combat_damage", "Non-Combat Damage"],
  ["combat_damage", "Combat Damage"],
  ["concede", "Concede"],
  ["unknown", "Unknown"],
] as const

export type WinCondition = (typeof WIN_CONDITIONS)[number][0]

export function winConditionLabel(value: WinCondition) {
  return WIN_CONDITIONS.find(([key]) => key === value)?.[1] ?? "Unknown"
}

export interface RecentGame {
  id: number
  played_at: string
  result: Seat["result"]
  deck: DeckSummary | null
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
export function canManagePlayer(
  viewer: { id: number; role: string } | undefined,
  player: PlayerSummary,
) {
  if (!viewer) return false
  return viewer.role === "admin" || player.user_id === null || player.user_id === viewer.id
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

export const getPlayers = () =>
  api<{ data: PlayerSummary[] }>("/api/players").then((body) => body.data)
export const getPlayer = (id: string) =>
  api<{ data: PlayerDetail }>(`/api/players/${id}`).then((body) => body.data)
/** Admin only: folds `sourceId` into `targetId` (seats, decks, identity) and deletes the source. */
export const mergePlayers = (sourceId: number, targetId: number) =>
  api<{ data: PlayerDetail }>(`/api/players/${sourceId}/merge`, {
    method: "POST",
    body: JSON.stringify({ target_id: targetId }),
  }).then((body) => body.data)
/** Admin only: makes `playerId` the account's player, merging the account's current player into it. */
export const linkUserPlayer = (userId: number, playerId: number) =>
  api<{ data: PlayerSummary }>(`/api/admin/users/${userId}/player`, {
    method: "PUT",
    body: JSON.stringify({ player_id: playerId }),
  }).then((body) => body.data)
export const getGame = (id: string) =>
  api<{ data: Game }>(`/api/games/${id}`).then((body) => body.data)
export const deleteGame = (id: number) => api<void>(`/api/games/${id}`, { method: "DELETE" })

/** Invalidates every cache derived from game, player, or deck records. */
export const invalidateGameRelated = (queryClient: QueryClient) =>
  Promise.all([
    queryClient.invalidateQueries({ queryKey: ["games"] }),
    queryClient.invalidateQueries({ queryKey: ["players"] }),
    queryClient.invalidateQueries({ queryKey: ["decks"] }),
    queryClient.invalidateQueries({ queryKey: ["stats"] }),
  ])

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
