export const CALENDAR_WEEKS = 52

export interface CalendarDay {
  date: Date
  key: string
  games: number
}

export function localDateKey(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${year}-${month}-${day}`
}

/** Counts ISO timestamps by their date in the browser's local time zone. */
export function activityByDay(gameTimes: string[]): Map<string, number> {
  const counts = new Map<string, number>()
  for (const value of gameTimes) {
    const key = localDateKey(new Date(value))
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  return counts
}

export function activityByWeekday(gameTimes: string[]): number[] {
  const counts = Array.from({ length: 7 }, () => 0)
  for (const value of gameTimes) counts[new Date(value).getDay()]! += 1
  return counts
}

export function activityByHour(gameTimes: string[]): number[] {
  const counts = Array.from({ length: 24 }, () => 0)
  for (const value of gameTimes) counts[new Date(value).getHours()]! += 1
  return counts
}

/** Returns 52 Sunday-to-Saturday columns, ending with the current local week. */
export function calendarWeeks(
  days: Map<string, number>,
  today = new Date(),
): (CalendarDay | null)[][] {
  const localToday = new Date(today.getFullYear(), today.getMonth(), today.getDate())
  const currentSunday = new Date(
    localToday.getFullYear(),
    localToday.getMonth(),
    localToday.getDate() - localToday.getDay(),
  )
  const firstSunday = new Date(
    currentSunday.getFullYear(),
    currentSunday.getMonth(),
    currentSunday.getDate() - (CALENDAR_WEEKS - 1) * 7,
  )

  return Array.from({ length: CALENDAR_WEEKS }, (_, week) =>
    Array.from({ length: 7 }, (_, weekday) => {
      const date = new Date(
        firstSunday.getFullYear(),
        firstSunday.getMonth(),
        firstSunday.getDate() + week * 7 + weekday,
      )
      if (date > localToday) return null
      const key = localDateKey(date)
      return { date, key, games: days.get(key) ?? 0 }
    }),
  )
}
