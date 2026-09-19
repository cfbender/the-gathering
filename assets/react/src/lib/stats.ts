import { api } from "@/lib/api"

export interface RecordStat {
  id: number | string
  name: string
  commander_name?: string
  games: number
  wins: number
  losses: number
  draws: number
  win_rate: number
}

export interface TrendPoint {
  date: string
  win_rate: number
}

export interface RecentStatGame {
  id: number
  played_at: string
  duration_minutes: number | null
  turns: number | null
  players: number
  result: "win" | "loss" | "draw" | null
  winner: { id: number; name: string } | null
}

export interface OverviewStats {
  games_count: number
  average_duration_minutes: number | null
  average_turns: number | null
  leaderboard: RecordStat[]
  games_by_month: { month: string; games: number }[]
  seat_win_rates: RecordStat[]
  color_win_rates: RecordStat[]
  commanders: RecordStat[]
  recent_games: RecentStatGame[]
}

export interface PlayerStats {
  record: RecordStat
  streaks: { current_wins: number; longest_wins: number }
  recent_form: ("win" | "loss" | "draw")[]
  win_rate_over_time: TrendPoint[]
  decks: RecordStat[]
  head_to_head: RecordStat[]
  seat_win_rates: RecordStat[]
  favorite_seat: number | null
  best_seat: number | null
  mvp_cards: { id: string | null; name: string; mentions: number }[]
}

export interface DeckStats {
  record: RecordStat
  average_duration_minutes: number | null
  average_turns: number | null
  opponents: RecordStat[]
  recent_games: RecentStatGame[]
  win_rate_over_time: TrendPoint[]
}

const data = <T>(path: string) => api<{ data: T }>(path).then((body) => body.data)
export const getOverviewStats = () => data<OverviewStats>("/api/stats/overview")
export const getPlayerStats = (id: string) => data<PlayerStats>(`/api/stats/players/${id}`)
export const getDeckStats = (id: string) => data<DeckStats>(`/api/stats/decks/${id}`)

export function linePoints(values: number[], width = 300, height = 100): string {
  if (!values.length) return ""
  if (values.length === 1) return `0,${height - (values.at(0)! / 100) * height}`
  return values
    .map(
      (value, index) =>
        `${(index / (values.length - 1)) * width},${height - (value / 100) * height}`,
    )
    .join(" ")
}
