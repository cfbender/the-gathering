import {
  activityByDay,
  activityByHour,
  activityByWeekday,
  calendarWeeks,
  type CalendarDay,
} from "@/lib/activity"

const weekdayLabels = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]

export function ActivityCalendar({ gameTimes }: { gameTimes: string[] }) {
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
            <div className="grid min-w-max grid-cols-[2rem_auto] gap-2">
              <div />
              <div
                className="text-base-content/45 grid h-5 gap-[3px] text-[10px]"
                style={{ gridTemplateColumns: "repeat(52, 0.875rem)" }}
              >
                {weeks.map((_, index) => (
                  <span key={index} className="whitespace-nowrap">
                    {monthLabels.get(index)}
                  </span>
                ))}
              </div>
              <div className="text-base-content/45 grid grid-rows-7 gap-[3px] text-[9px] leading-3">
                {weekdayLabels.map((label, index) => (
                  <span key={label}>{index % 2 === 1 ? label : ""}</span>
                ))}
              </div>
              <div
                className="grid grid-flow-col grid-rows-7 gap-[3px]"
                style={{ gridTemplateColumns: "repeat(52, 0.875rem)" }}
              >
                {weeks.flatMap((week, weekIndex) =>
                  week.map((day, weekday) => (
                    <ActivityDay key={`${weekIndex}-${weekday}`} day={day} max={maxDay} />
                  )),
                )}
              </div>
            </div>
          </div>

          <div className="mt-5 grid gap-6 lg:grid-cols-2">
            <BreakdownBars labels={weekdayLabels} values={weekdays} title="By weekday" />
            <BreakdownBars
              labels={Array.from({ length: 24 }, (_, hour) =>
                hour === 0 ? "12a" : hour < 12 ? `${hour}a` : hour === 12 ? "12p" : `${hour - 12}p`,
              )}
              values={hours}
              title="By hour"
              compact
            />
          </div>
        </>
      )}
    </section>
  )
}

function ActivityDay({ day, max }: { day: CalendarDay | null; max: number }) {
  if (!day) return <span className="size-3.5" />
  const intensity = day.games === 0 ? 0 : 20 + (day.games / max) * 80
  return (
    <span
      className="border-base-300 size-3.5 rounded-[3px] border"
      title={`${day.date.toLocaleDateString()}: ${day.games} ${day.games === 1 ? "game" : "games"}`}
      style={{
        background:
          day.games === 0
            ? "var(--color-base-300)"
            : `color-mix(in oklab, var(--color-primary) ${intensity}%, var(--color-base-200))`,
      }}
    />
  )
}

function BreakdownBars({
  labels,
  values,
  title,
  compact = false,
}: {
  labels: string[]
  values: number[]
  title: string
  compact?: boolean
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
          <div key={labels[index]} className="flex h-full min-w-0 flex-col justify-end gap-1">
            <div
              className="bg-primary/70 min-h-px rounded-t-sm"
              style={{ height: `${(value / max) * 100}%` }}
              title={`${labels[index]}: ${value} games`}
            />
            <span className="text-base-content/45 truncate text-center text-[9px]">
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
