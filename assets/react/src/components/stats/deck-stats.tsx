import { useQuery } from "@tanstack/react-query"
import { Clock3, RotateCcw, Target } from "lucide-react"
import { BarChart, LineChart } from "./charts"
import { StatCard } from "./stat-card"
import { StatsRangeToggle } from "./stats-range-toggle"
import { getDeckStats, sinceLabel, statsQueryKey } from "@/lib/stats"
import { statsRangeDetails, useStatsRange } from "@/lib/stats-range"

export function DeckStats({ deckId }: { deckId: string }) {
  const { range, params } = useStatsRange()
  const query = useQuery({
    queryKey: statsQueryKey(params, "decks", deckId),
    queryFn: () => getDeckStats(deckId, params),
  })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return null
  const stats = query.data
  const since = sinceLabel(stats.detailed_stats_from)
  return (
    <section className="space-y-4" aria-labelledby="deck-stats-heading">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-primary text-xs font-bold tracking-wider uppercase">Deck analytics</p>
          <h2 id="deck-stats-heading" className="text-2xl font-black">
            Performance
          </h2>
        </div>
        <StatsRangeToggle className="shrink-0" />
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        <StatCard
          label="Win rate"
          value={`${stats.record.win_rate}%`}
          detail={`${stats.record.wins}–${stats.record.losses}–${stats.record.draws} · ${statsRangeDetails[range]}`}
          icon={<Target className="size-4" />}
        />
        <StatCard
          label="Avg. length"
          value={stats.average_duration_minutes ? `${stats.average_duration_minutes}m` : "—"}
          detail={since}
          icon={<Clock3 className="size-4" />}
        />
        <StatCard
          label="Avg. turns"
          value={stats.average_turns ?? "—"}
          detail={since}
          icon={<RotateCcw className="size-4" />}
        />
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
          <h3 className="mb-4 font-bold">Win rate over time</h3>
          <LineChart points={stats.win_rate_over_time} />
        </div>
        <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
          <h3 className="mb-4 font-bold">Opponents faced</h3>
          <BarChart rows={stats.opponents.slice(0, 6)} value="games" />
        </div>
      </div>
    </section>
  )
}
