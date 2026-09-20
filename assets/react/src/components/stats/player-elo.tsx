import { TrendingUp } from "lucide-react"
import { EloChart } from "./elo-chart"
import { StatCard } from "./stat-card"
import type { PlayerStats } from "@/lib/stats"

export function EloStatCard({ elo, className }: { elo: PlayerStats["elo"]; className?: string }) {
  return (
    <StatCard
      className={className}
      label="Elo rating"
      value={elo ? elo.rating : "—"}
      detail={elo ? `#${elo.rank} of ${elo.players} · peak ${elo.peak}` : "no rated games yet"}
      icon={<TrendingUp className="size-4" />}
    />
  )
}

export function EloTrendCard({ elo }: { elo: PlayerStats["elo"] }) {
  return (
    <div className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5">
      <h3 className="mb-4 font-bold">Rating over time</h3>
      {elo ? (
        <EloChart
          series={[{ id: 0, name: "Rating", rating: elo.rating, history: elo.history }]}
          showLegend={false}
        />
      ) : (
        <p className="text-base-content/50 text-sm">No rating history yet.</p>
      )}
    </div>
  )
}
