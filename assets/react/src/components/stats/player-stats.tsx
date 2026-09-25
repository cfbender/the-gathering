import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { Flame, Medal, Target } from "lucide-react"
import { CardArtBackground } from "@/components/card-art-background"
import { CardHover } from "@/components/card-hover"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { BarChart, LineChart } from "./charts"
import { ColorSection } from "./color-section"
import { ColorRadar } from "./color-radar"
import { ColorWheel } from "./color-wheel"
import { GameLengths } from "./game-lengths"
import { EloStatCard, EloTrendCard } from "./player-elo"
import { Rivalries } from "./rivalries"
import { StatCard } from "./stat-card"
import { StatsRangeToggle } from "./stats-range-toggle"
import { PlayerWinConditions } from "./win-conditions"
import { byWinRateThenName, getPlayerStats, sinceLabel, statsQueryKey } from "@/lib/stats"
import { statsRangeDetails, useStatsRange } from "@/lib/stats-range"
import { detailedScope, gamesLink } from "@/features/games/game-filters"

export function PlayerStats({ playerId }: { playerId: string }) {
  const { range, params } = useStatsRange()
  const query = useQuery({
    queryKey: statsQueryKey(params, "players", playerId),
    queryFn: () => getPlayerStats(playerId, params),
  })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return null
  const stats = query.data
  const since = sinceLabel(stats.detailed_stats_from)
  const games = { date_from: params.date_from, player_id: Number(playerId) }
  const activeDecks = stats.decks.filter((deck) => !deck.retired)
  const retiredDecks = stats.decks.length - activeDecks.length
  return (
    <section className="space-y-4" aria-labelledby="player-stats-heading">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="text-primary text-xs font-bold tracking-wider uppercase">Performance</p>
          <h2 id="player-stats-heading" className="text-2xl font-black">
            At the table
          </h2>
        </div>
        <StatsRangeToggle className="shrink-0" />
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        <StatCard
          label="Win rate"
          value={`${stats.record.win_rate}%`}
          detail={`${stats.record.wins}–${stats.record.losses}–${stats.record.draws} · ${statsRangeDetails[range]}`}
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
      <PlayerWinConditions
        wins={stats.win_conditions}
        losses={stats.loss_conditions}
        games={games}
      />
      <div className="grid gap-4 lg:grid-cols-2">
        <div className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5">
          <h3 className="mb-4 font-bold">Win rate over time</h3>
          <LineChart points={stats.win_rate_over_time} className="h-48" />
        </div>
        <EloTrendCard elo={stats.elo} />
      </div>
      <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
        <div className="mb-4 flex items-baseline justify-between gap-3">
          <h3 className="font-bold">Deck performance</h3>
          <span className="text-base-content/60 text-xs">
            {retiredDecks > 0
              ? `Click a deck to see its games · ${retiredDecks} retired ${retiredDecks === 1 ? "deck" : "decks"} hidden`
              : "Click a deck to see its games"}
          </span>
        </div>
        <BarChart
          rows={byWinRateThenName(activeDecks)}
          columns={2}
          renderLabel={(row) => (
            <>
              {row.name}
              {row.commander_name && !row.name.includes(row.commander_name) && (
                <span className="text-base-content/70 font-normal"> · {row.commander_name}</span>
              )}
            </>
          )}
          linkTo={(row) =>
            gamesLink(games, {
              player_id: games.player_id,
              commander: row.commander_name ?? row.name,
            })
          }
        />
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <ColorSection rows={stats.color_win_rates} eyebrow="Their colors" games={games} />
        <ColorWheel rows={stats.color_exposure} eyebrow="Their color mix" games={games} />
      </div>
      <ColorRadar rows={stats.color_exposure} games={games} />
      <Rivalries headToHead={stats.head_to_head} commanders={stats.rival_commanders} />
      <GameLengths
        gameLengths={stats.game_lengths}
        averageDuration={stats.average_duration_minutes}
        averageTurns={stats.average_turns}
        since={since}
        games={detailedScope(games, stats.detailed_stats_from)}
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
              <CardHover
                key={`${card.id ?? card.name}-${card.name}`}
                id={card.image_url ? null : card.id}
                name={card.name}
                gameChanger={card.game_changer}
                imageUrl={card.image_url}
                artCropUrl={card.art_crop_url}
              >
                <div className="border-base-300 bg-base-200 relative w-full cursor-help overflow-hidden rounded-lg border px-4 py-3">
                  <CardArtBackground imageUrl={card.art_crop_url} />
                  <div className="text-base-content relative z-10">
                    <strong className="block truncate">{card.name}</strong>
                    <GameChangerBadge gameChanger={card.game_changer} />
                    <span className="text-base-content/80 text-xs">
                      {card.mentions} {card.mentions === 1 ? "mention" : "mentions"}
                    </span>
                  </div>
                </div>
              </CardHover>
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
