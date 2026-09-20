import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { Crown } from "lucide-react"
import { useState } from "react"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import {
  getCommanderStats,
  sortByMetric,
  type ColorMetric,
  type CommanderSummary,
} from "@/lib/stats"

export const Route = createFileRoute("/commanders/")({ component: CommandersPage })

const metricLabels: Record<ColorMetric, string> = {
  games: "Most played",
  win_rate: "Win rate",
}

function CommandersPage() {
  const query = useQuery({ queryKey: ["stats", "commanders"], queryFn: () => getCommanderStats() })
  const [metric, setMetric] = useState<ColorMetric>("games")
  const rows = sortByMetric(query.data ?? [], metric) as CommanderSummary[]

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="The meta"
        title="Commanders"
        description="Every commander that has hit the table, across all pilots and decks."
        actions={
          query.data && (
            <ToggleGroup
              type="single"
              value={metric}
              onValueChange={(value) => value && setMetric(value as ColorMetric)}
              aria-label="Sort commanders by"
              className="join"
            >
              {(Object.keys(metricLabels) as ColorMetric[]).map((value) => (
                <ToggleGroupItem
                  key={value}
                  value={value}
                  className={cn(
                    "btn btn-sm join-item",
                    metric === value ? "btn-primary" : "btn-ghost",
                  )}
                >
                  {metricLabels[value]}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          )
        }
      />
      {query.isPending && <span className="loading loading-spinner" />}
      {query.isError && <div className="alert alert-error">Could not load commanders.</div>}
      {query.data && rows.length === 0 && (
        <EmptyPanel
          icon={<Crown className="size-10" />}
          title={metric === "games" ? "No commanders yet" : "Not enough games yet"}
          description={
            metric === "games"
              ? "Commanders appear here once a game with a deck is logged."
              : "A commander needs a couple of games before its win rate is ranked."
          }
          action={
            <Link to="/games/new" className="btn btn-primary">
              Log a game
            </Link>
          }
        />
      )}
      <ol className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {rows.map((commander, index) => (
          <li key={commander.id} className="min-w-0">
            <Link
              to="/commanders/$commanderId"
              params={{ commanderId: commander.id }}
              className="card group border-base-300 bg-base-200 hover:border-primary relative block overflow-hidden border transition-all hover:-translate-y-0.5 hover:shadow-xl"
            >
              <CardArtBackground imageUrl={commander.art_crop_url} interactive />
              <div className="card-body text-base-content relative z-10 gap-2 p-5">
                <div className="flex items-start justify-between gap-3">
                  <span className="text-primary text-xs font-bold uppercase">#{index + 1}</span>
                  <ColorIdentity colors={commander.color_identity ?? ""} />
                </div>
                <h2 className="truncate text-xl font-bold" title={commander.name}>
                  {commander.name}
                </h2>
                <dl className="border-base-300/70 bg-base-100/75 rounded-box grid grid-cols-3 gap-2 border px-3 py-2 text-sm shadow-sm backdrop-blur">
                  <div>
                    <dt className="text-base-content/70 text-xs uppercase">Games</dt>
                    <dd className="font-bold tabular-nums">{commander.games}</dd>
                  </div>
                  <div>
                    <dt className="text-base-content/70 text-xs uppercase">Win rate</dt>
                    <dd className="text-primary font-bold tabular-nums">{commander.win_rate}%</dd>
                  </div>
                  <div>
                    <dt className="text-base-content/70 text-xs uppercase">Pilots</dt>
                    <dd className="font-bold tabular-nums">{commander.pilots}</dd>
                  </div>
                </dl>
              </div>
            </Link>
          </li>
        ))}
      </ol>
    </div>
  )
}
