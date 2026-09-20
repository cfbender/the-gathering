import { TrendingUp } from "lucide-react"
import { EloChart } from "./elo-chart"
import { StatCard } from "./stat-card"
import type { PlayerStats } from "@/lib/stats"

export function PlayerElo({ elo }: { elo: PlayerStats["elo"] }) {
  if (!elo) {
    return (
      <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
        <h3 className="font-bold">Elo rating</h3>
        <p className="text-base-content/50 mt-1 text-sm">No Elo rating history yet.</p>
      </div>
    )
  }

  const series = [{ id: 0, name: "Rating", rating: elo.rating, history: elo.history }]

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(13rem,0.55fr)_minmax(0,1.45fr)]">
      <div className="self-start">
        <StatCard
          label="Elo rating"
          value={elo.rating}
          detail={`#${elo.rank} of ${elo.players} · peak ${elo.peak}`}
          icon={<TrendingUp className="size-4" />}
        />
      </div>
      <div className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5">
        <h3 className="mb-3 font-bold">Rating over time</h3>
        <EloChart series={series} showLegend={false} />
      </div>
    </div>
  )
}
