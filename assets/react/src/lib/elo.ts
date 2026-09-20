import type { OverviewStats } from "@/lib/stats"

export type EloSeries = Pick<OverviewStats["elo"][number], "id" | "name" | "rating" | "history">

export interface EloChartBounds {
  startDate: string
  endDate: string
  minRating: number
  maxRating: number
}

const dateValue = (date: string) => Date.parse(`${date}T00:00:00Z`)

export function selectEloSeries<T extends OverviewStats["elo"][number]>(
  players: T[],
  minGames: number,
  limit = 6,
): T[] {
  return players
    .filter((player) => player.games >= minGames)
    .sort((a, b) => b.rating - a.rating || b.games - a.games || a.name.localeCompare(b.name))
    .slice(0, limit)
}

export function eloChartBounds(series: EloSeries[]): EloChartBounds | null {
  const points = series.flatMap((player) => player.history)
  if (points.length === 0) return null

  const dates = points.map((point) => point.date).sort()
  const ratings = points.map((point) => point.rating)
  const minRating = Math.min(1000, ...ratings)
  const maxRating = Math.max(1000, ...ratings)

  return {
    startDate: dates[0]!,
    endDate: dates.at(-1)!,
    minRating: minRating === maxRating ? minRating - 1 : minRating,
    maxRating: minRating === maxRating ? maxRating + 1 : maxRating,
  }
}

export function dateToX(date: string, startDate: string, endDate: string, width: number): number {
  const start = dateValue(startDate)
  const span = dateValue(endDate) - start
  return span === 0 ? width / 2 : ((dateValue(date) - start) / span) * width
}

export function ratingToY(rating: number, minRating: number, maxRating: number, height: number) {
  const span = maxRating - minRating
  return span === 0 ? height / 2 : height - ((rating - minRating) / span) * height
}

export function buildEloPath(
  history: EloSeries["history"],
  bounds: EloChartBounds,
  width: number,
  height: number,
): string {
  return history
    .map((point) => {
      const x = dateToX(point.date, bounds.startDate, bounds.endDate, width)
      const y = ratingToY(point.rating, bounds.minRating, bounds.maxRating, height)
      return `${x},${y}`
    })
    .join(" ")
}
