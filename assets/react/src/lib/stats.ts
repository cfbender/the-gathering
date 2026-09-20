import { api } from "@/lib/api"

export interface RecordStat {
  id: number | string
  name: string
  commander_name?: string
  art_crop_url?: string | null
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
  /** ISO date; seat, timing, and MVP figures only include games from this date on. */
  detailed_stats_from: string | null
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
  detailed_stats_from: string | null
  record: RecordStat
  streaks: { current_wins: number; longest_wins: number }
  recent_form: ("win" | "loss" | "draw")[]
  win_rate_over_time: TrendPoint[]
  decks: RecordStat[]
  head_to_head: RecordStat[]
  seat_win_rates: RecordStat[]
  favorite_seat: number | null
  best_seat: number | null
  mvp_cards: { id: string | null; name: string; mentions: number; art_crop_url: string | null }[]
}

export interface DeckStats {
  detailed_stats_from: string | null
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

/** Label for figures limited by the administrator's detailed-stats cutoff. */
export function sinceLabel(detailedStatsFrom: string | null, fallback?: string) {
  if (!detailedStatsFrom) return fallback
  const date = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(
    new Date(`${detailedStatsFrom}T00:00:00`),
  )
  return `since ${date}`
}

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
