import { describe, expect, it } from "vite-plus/test"
import {
  CALENDAR_WEEKS,
  activityByDay,
  activityByHour,
  activityByWeekday,
  calendarWeeks,
  localDateKey,
} from "@/lib/activity"

describe("activity binning", () => {
  it("uses local dates, weekdays, and hours", () => {
    const evening = new Date(2026, 6, 11, 23, 30)
    const morning = new Date(2026, 6, 12, 8, 15)
    const gameTimes = [evening.toISOString(), morning.toISOString(), morning.toISOString()]

    expect(activityByDay(gameTimes)).toEqual(
      new Map([
        [localDateKey(evening), 1],
        [localDateKey(morning), 2],
      ]),
    )
    expect(activityByWeekday(gameTimes)[evening.getDay()]).toBe(1)
    expect(activityByWeekday(gameTimes)[morning.getDay()]).toBe(2)
    expect(activityByHour(gameTimes)[evening.getHours()]).toBe(1)
    expect(activityByHour(gameTimes)[morning.getHours()]).toBe(2)
  })

  it("lays out Sunday-aligned weeks and omits future days", () => {
    const today = new Date(2026, 6, 15, 18)
    const counts = new Map([[localDateKey(today), 4]])
    const weeks = calendarWeeks(counts, today)
    const currentWeek = weeks.at(-1)!

    expect(weeks).toHaveLength(CALENDAR_WEEKS)
    expect(currentWeek[0]?.date.getDay()).toBe(0)
    expect(currentWeek[today.getDay()]).toMatchObject({ key: localDateKey(today), games: 4 })
    expect(currentWeek.slice(today.getDay() + 1).every((day) => day === null)).toBe(true)
  })
})
