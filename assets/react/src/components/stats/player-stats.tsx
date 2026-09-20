import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { Flame, Medal, Target } from "lucide-react"
import { CardArtBackground } from "@/components/card-art-background"
import { BarChart, LineChart } from "./charts"
import { ColorSection } from "./color-section"
import { ColorRadar } from "./color-radar"
import { ColorWheel } from "./color-wheel"
import { GameLengths } from "./game-lengths"
import { EloStatCard, EloTrendCard } from "./player-elo"
import { Rivalries } from "./rivalries"
import { StatCard } from "./stat-card"
import { byWinRateThenName, getPlayerStats, sinceLabel } from "@/lib/stats"

export function PlayerStats({ playerId }: { playerId: string }) {
  const query = useQuery({
    queryKey: ["stats", "players", playerId],
    queryFn: () => getPlayerStats(playerId),
  })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return null
  const stats = query.data
  const since = sinceLabel(stats.detailed_stats_from)
  return (
    <section className="space-y-4" aria-labelledby="player-stats-heading">
      <div>
        <p className="text-primary text-xs font-bold tracking-wider uppercase">Performance</p>
        <h2 id="player-stats-heading" className="text-2xl font-black">
          At the table
        </h2>
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        <StatCard
          label="Win rate"
          value={`${stats.record.win_rate}%`}
          detail={`${stats.record.wins}–${stats.record.losses}–${stats.record.draws}`}
          icon={<Target className="size-4" />}
        />
        <StatCard
          label="Current streak"
          value={stats.streaks.current_wins}
          detail="consecutive wins"
          icon={<Flame className="size-4" />}
        />
        <StatCard
          label="Best streak"
          value={stats.streaks.longest_wins}
          detail="consecutive wins"
          icon={<Medal className="size-4" />}
        />
        <StatCard
          label="Best seat"
          value={stats.best_seat ? `#${stats.best_seat}` : "—"}
          detail={[stats.favorite_seat && `usually seat #${stats.favorite_seat}`, since]
            .filter(Boolean)
            .join(", ")}
        />
        <EloStatCard elo={stats.elo} className="col-span-2 sm:col-span-1" />
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <div className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5">
          <h3 className="mb-4 font-bold">Win rate over time</h3>
          <LineChart points={stats.win_rate_over_time} className="h-48" />
        </div>
        <EloTrendCard elo={stats.elo} />
      </div>
      <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
        <h3 className="mb-4 font-bold">Deck performance</h3>
        <BarChart rows={byWinRateThenName(stats.decks)} columns={2} />
      </div>
      <div className="grid gap-4 lg:grid-cols-3">
        <ColorSection rows={stats.color_win_rates} eyebrow="Their colors" />
        <ColorRadar rows={stats.color_exposure} />
        <ColorWheel rows={stats.color_exposure} eyebrow="Their color mix" />
      </div>
      <Rivalries headToHead={stats.head_to_head} commanders={stats.rival_commanders} />
      <GameLengths
        gameLengths={stats.game_lengths}
        averageDuration={stats.average_duration_minutes}
        averageTurns={stats.average_turns}
        since={since}
      />
      {stats.mvp_cards.length > 0 && (
        <div>
          <h3 className="mb-3 font-bold">
            MVP cards
            {since && (
              <span className="text-base-content/50 ml-2 text-sm font-medium">{since}</span>
            )}
          </h3>
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
            {stats.mvp_cards.map((card) => (
              <div
                key={`${card.id ?? card.name}-${card.name}`}
                className="border-base-300 bg-base-200 relative overflow-hidden rounded-lg border px-4 py-3"
              >
                <CardArtBackground imageUrl={card.art_crop_url} />
                <div className="text-base-content relative z-10">
                  <strong className="block truncate">{card.name}</strong>
                  <span className="text-base-content/80 text-xs">
                    {card.mentions} {card.mentions === 1 ? "mention" : "mentions"}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
      <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
        <h3 className="mb-3 font-bold">Head to head</h3>
        <div className="grid gap-2 sm:grid-cols-2">
          {stats.head_to_head.map((opponent) => (
            <Link
              key={opponent.id}
              to="/players/$playerId"
              params={{ playerId: String(opponent.id) }}
              className="border-base-300 flex items-center justify-between rounded-lg border p-3"
            >
              <span>
                {opponent.name}
                <small className="text-base-content/50 block">{opponent.games} shared games</small>
              </span>
              <strong>
                {opponent.wins}–{opponent.losses}
              </strong>
            </Link>
          ))}
        </div>
      </div>
    </section>
  )
}
