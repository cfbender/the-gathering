import { api } from "@/lib/api"

export interface RecordCounts {
  games: number
  wins: number
  losses: number
  draws: number
  win_rate: number
}

export interface NamedRecordRow extends RecordCounts {
  id: number | string
  name: string
  commander_name?: string
  art_crop_url?: string | null
}

export interface HeadToHead {
  id: number
  name: string
  games: number
  wins: number
  losses: number
  draws: number
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
  leaderboard: NamedRecordRow[]
  games_by_month: { month: string; games: number }[]
  seat_win_rates: NamedRecordRow[]
  color_win_rates: NamedRecordRow[]
  /** The top eight rows of `getCommanderStats()`, keyed by the same canonical IDs. */
  commanders: CommanderSummary[]
  recent_games: RecentStatGame[]
}

export interface PlayerStats {
  detailed_stats_from: string | null
  record: RecordCounts
  streaks: { current_wins: number; longest_wins: number }
  recent_form: ("win" | "loss" | "draw")[]
  win_rate_over_time: TrendPoint[]
  decks: NamedRecordRow[]
  /** This player's seats grouped by deck color identity; same shape as the overview's. */
  color_win_rates: NamedRecordRow[]
  head_to_head: HeadToHead[]
  seat_win_rates: NamedRecordRow[]
  favorite_seat: number | null
  best_seat: number | null
  mvp_cards: { id: string | null; name: string; mentions: number; art_crop_url: string | null }[]
}

export interface DeckStats {
  detailed_stats_from: string | null
  record: RecordCounts
  average_duration_minutes: number | null
  average_turns: number | null
  opponents: NamedRecordRow[]
  recent_games: RecentStatGame[]
  win_rate_over_time: TrendPoint[]
}

export interface CommanderSummary extends RecordCounts {
  /** Scryfall card ID, or the card name when the card is missing from the catalog. */
  id: string
  name: string
  art_crop_url: string | null
  color_identity: string | null
  pilots: number
  decks: number
  last_played_at: string
}

export interface CommanderStats {
  commander: {
    id: string
    name: string
    art_crop_url: string | null
    color_identity: string | null
  }
  record: RecordCounts
  pilots: NamedRecordRow[]
  decks: NamedRecordRow[]
  partners: NamedRecordRow[]
  opponents: NamedRecordRow[]
  win_rate_over_time: TrendPoint[]
  recent_games: RecentStatGame[]
}

const data = <T>(path: string) => api<{ data: T }>(path).then((body) => body.data)
export const getOverviewStats = () => data<OverviewStats>("/api/stats/overview")
export const getPlayerStats = (id: string) => data<PlayerStats>(`/api/stats/players/${id}`)
export const getDeckStats = (id: string) => data<DeckStats>(`/api/stats/decks/${id}`)
export const getCommanderStats = () => data<CommanderSummary[]>("/api/stats/commanders")
export const getCommanderDetail = (id: string) =>
  data<CommanderStats>(`/api/stats/commanders/${encodeURIComponent(id)}`)

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

/** Players need this many games before they appear in the playgroup leaderboard. */
export const LEADERBOARD_MIN_GAMES = 2

/** Leaderboard rows with enough games, best win rate first (more games breaks ties). */
export function leaderboardRows<T extends NamedRecordRow>(
  rows: T[],
  minGames = LEADERBOARD_MIN_GAMES,
): T[] {
  return rows
    .filter((row) => row.games >= minGames)
    .sort((a, b) => b.win_rate - a.win_rate || b.games - a.games)
}

/** Best win rate first; ties fall back to name, so equal decks read alphabetically. */
export function byWinRateThenName<T extends NamedRecordRow>(rows: T[]): T[] {
  return [...rows].sort(
    (a, b) =>
      b.win_rate - a.win_rate || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  )
}

export type ColorMetric = "games" | "win_rate"

/**
 * Rows ordered by the chosen metric, highest first; the other metric breaks ties.
 * Win-rate ranking skips rows below the game floor so a single win cannot top the chart.
 */
export function sortByMetric<T extends NamedRecordRow>(
  rows: T[],
  metric: ColorMetric,
  minGames = LEADERBOARD_MIN_GAMES,
): T[] {
  const other: ColorMetric = metric === "games" ? "win_rate" : "games"
  return rows
    .filter((row) => metric === "games" || row.games >= minGames)
    .sort((a, b) => b[metric] - a[metric] || b[other] - a[other])
}
