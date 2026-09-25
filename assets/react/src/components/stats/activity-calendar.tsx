import { Link } from "@tanstack/react-router"
import { gamesLink, hourLabel, type GamesLinkScope } from "@/features/games/game-filters"
import {
  activityByDay,
  activityByHour,
  activityByWeekday,
  calendarWeeks,
  type CalendarDay,
} from "@/lib/activity"

const weekdayLabels = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]

/** With `games`, days, weekdays, and hours link to their games in the viewer's time zone. */
export function ActivityCalendar({
  gameTimes,
  games,
}: {
  gameTimes: string[]
  games?: GamesLinkScope
}) {
  const weeks = calendarWeeks(activityByDay(gameTimes))
  const weekdays = activityByWeekday(gameTimes)
  const hours = activityByHour(gameTimes)
  const maxDay = Math.max(...weeks.flatMap((week) => week.map((day) => day?.games ?? 0)), 1)
  const monthLabels = calendarMonthLabels(weeks)

  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <p className="text-primary text-xs font-bold uppercase">When we play</p>
      <h2 className="text-xl font-bold">Table activity</h2>
      <p className="text-base-content/55 mt-1 text-sm">
        Games by local date over the last 52 weeks.
      </p>

      {gameTimes.length === 0 ? (
        <p className="text-base-content/50 mt-5 text-sm">No games recorded yet.</p>
      ) : (
        <>
          <div className="mt-5 overflow-x-auto pb-2">
            <div className="grid min-w-[57rem] grid-cols-[2rem_minmax(0,1fr)] gap-2">
              <div />
              <div
                className="text-base-content/45 grid h-5 gap-[3px] text-[10px]"
                style={{ gridTemplateColumns: "repeat(52, minmax(0, 1fr))" }}
              >
                {weeks.map((_, index) => (
                  <span key={index} className="whitespace-nowrap">
                    {monthLabels.get(index)}
                  </span>
                ))}
              </div>
              <div className="text-base-content/45 grid grid-rows-7 items-center gap-[3px] text-[9px] leading-3">
                {weekdayLabels.map((label, index) => (
                  <span key={label}>{index % 2 === 1 ? label : ""}</span>
                ))}
              </div>
              <div
                className="grid grid-flow-col grid-rows-7 gap-[3px]"
                style={{ gridTemplateColumns: "repeat(52, minmax(0, 1fr))" }}
              >
                {weeks.flatMap((week, weekIndex) =>
                  week.map((day, weekday) => (
                    <ActivityDay
                      key={`${weekIndex}-${weekday}`}
                      day={day}
                      max={maxDay}
                      games={games}
                    />
                  )),
                )}
              </div>
            </div>
          </div>

          <div className="mt-5 grid gap-6 lg:grid-cols-2">
            <BreakdownBars
              labels={weekdayLabels}
              values={weekdays}
              title="By weekday"
              link={games && ((weekday) => gamesLink(games, { weekday }))}
            />
            <BreakdownBars
              labels={Array.from({ length: 24 }, (_, hour) => hourLabel(hour).replace("m", ""))}
              values={hours}
              title="By hour"
              compact
              link={games && ((hour) => gamesLink(games, { hour }))}
            />
          </div>
        </>
      )}
    </section>
  )
}

function ActivityDay({
  day,
  max,
  games,
}: {
  day: CalendarDay | null
  max: number
  games?: GamesLinkScope
}) {
  if (!day) return <span className="aspect-square w-full" />
  const intensity = day.games === 0 ? 0 : 20 + (day.games / max) * 80
  const title = `${day.date.toLocaleDateString()}: ${day.games} ${day.games === 1 ? "game" : "games"}`
  const props = {
    className: "border-base-300 block aspect-square w-full rounded-[3px] border",
    title,
    style: {
      background:
        day.games === 0
          ? "var(--color-base-300)"
          : `color-mix(in oklab, var(--color-primary) ${intensity}%, var(--color-base-200))`,
    },
  }
  if (!games || day.games === 0) return <span {...props} />
  // A single day stands on its own, so it ignores the stats range start.
  return (
    <Link
      {...gamesLink({}, { date_from: day.key, date_to: day.key })}
      {...props}
      className={`${props.className} hover:ring-primary hover:ring-2`}
      aria-label={`${title}. Show games`}
    />
  )
}

function BreakdownBars({
  labels,
  values,
  title,
  compact = false,
  link,
}: {
  labels: string[]
  values: number[]
  title: string
  compact?: boolean
  /** Link options for the bar at `index`, when bars should open their games. */
  link?: (index: number) => ReturnType<typeof gamesLink>
}) {
  const max = Math.max(...values, 1)
  return (
    <div>
      <h3 className="text-base-content/65 mb-2 text-xs font-bold uppercase">{title}</h3>
      <div
        className="grid h-20 items-end gap-1"
        style={{ gridTemplateColumns: `repeat(${values.length}, minmax(0, 1fr))` }}
      >
        {values.map((value, index) => (
          <div key={labels[index]} className="flex h-full min-w-0 flex-col gap-1">
            <div className="flex min-h-0 flex-1 items-end">
              {link && value > 0 ? (
                <Link
                  {...link(index)}
                  className="bg-primary/70 hover:bg-primary block min-h-px w-full rounded-t-sm transition-colors"
                  style={{ height: `${(value / max) * 100}%` }}
                  title={`${labels[index]}: ${value} games`}
                  aria-label={`${labels[index]}: ${value} games. Show games`}
                />
              ) : (
                <div
                  className="bg-primary/70 min-h-px w-full rounded-t-sm"
                  style={{ height: `${(value / max) * 100}%` }}
                  title={`${labels[index]}: ${value} games`}
                />
              )}
            </div>
            <span className="text-base-content/45 h-3 shrink-0 truncate text-center text-[9px] leading-3">
              {!compact || index % 6 === 0 ? labels[index] : ""}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

function calendarMonthLabels(weeks: (CalendarDay | null)[][]): Map<number, string> {
  const labels = new Map<number, string>()
  let previousMonth = -1
  let previousIndex = -1
  weeks.forEach((week, index) => {
    const firstDay = week.find(Boolean)
    if (firstDay && firstDay.date.getMonth() !== previousMonth) {
      if (index - previousIndex < 3) labels.delete(previousIndex)
      labels.set(index, firstDay.date.toLocaleDateString(undefined, { month: "short" }))
      previousMonth = firstDay.date.getMonth()
      previousIndex = index
    }
  })
  return labels
}
