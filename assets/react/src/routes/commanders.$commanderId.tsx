import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute, type LinkProps } from "@tanstack/react-router"
import { Gamepad2, Layers, Target, Users } from "lucide-react"
import type { ReactNode } from "react"
import { PageHeader } from "@/components/app-shell"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { BarChart, LineChart } from "@/components/stats/charts"
import { CommanderRivalries } from "@/components/stats/rivalries"
import { StatCard } from "@/components/stats/stat-card"
import { formatDate } from "@/features/games/games"
import { getCommanderDetail, type NamedRecordRow } from "@/lib/stats"

export const Route = createFileRoute("/commanders/$commanderId")({ component: CommanderPage })

function CommanderPage() {
  const { commanderId } = Route.useParams()
  const query = useQuery({
    queryKey: ["stats", "commanders", commanderId],
    queryFn: () => getCommanderDetail(commanderId),
  })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return <div className="alert alert-error">Commander not found.</div>
  const stats = query.data
  const { commander, record } = stats

  return (
    <div className="flex flex-col gap-8">
      <PageHeader
        eyebrow={<Link to="/commanders">Commanders</Link>}
        title={commander.name}
        backgroundImageUrl={commander.art_crop_url}
      >
        <ColorIdentity colors={commander.color_identity ?? ""} className="mt-4 text-lg" />
      </PageHeader>

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard
          label="Win rate"
          value={`${record.win_rate}%`}
          detail={`${record.wins}–${record.losses}–${record.draws}`}
          icon={<Target className="size-4" />}
        />
        <StatCard label="Games" value={record.games} icon={<Gamepad2 className="size-4" />} />
        <StatCard label="Pilots" value={stats.pilots.length} icon={<Users className="size-4" />} />
        <StatCard label="Decks" value={stats.decks.length} icon={<Layers className="size-4" />} />
      </div>

      <CommanderRivalries opponents={stats.opponents} />

      <div className="grid gap-4 lg:grid-cols-2">
        <Panel title="Win rate over time">
          <LineChart points={stats.win_rate_over_time} />
        </Panel>
        <Panel title="Pilots">
          <RecordLinks
            rows={stats.pilots}
            link={(row) => ({ to: "/players/$playerId", params: { playerId: String(row.id) } })}
          />
        </Panel>
        <Panel title="Decks">
          <RecordLinks
            rows={stats.decks}
            link={(row) => ({ to: "/decks/$deckId", params: { deckId: String(row.id) } })}
          />
        </Panel>
        <Panel title="Opponents faced">
          {stats.opponents.length ? (
            <BarChart rows={stats.opponents.slice(0, 6)} value="games" />
          ) : (
            <Empty>No opponents recorded.</Empty>
          )}
        </Panel>
        {stats.partners.length > 0 && (
          <Panel title="Paired with">
            <RecordLinks
              rows={stats.partners}
              link={(row) => ({
                to: "/commanders/$commanderId",
                params: { commanderId: String(row.id) },
              })}
            />
          </Panel>
        )}
      </div>

      <section>
        <h2 className="mb-3 text-xl font-bold">Recent games</h2>
        <div className="divide-base-300 border-base-300 bg-base-200 divide-y rounded-lg border">
          {stats.recent_games.map((game) => (
            <Link
              key={game.id}
              to="/games/$gameId"
              params={{ gameId: String(game.id) }}
              className="flex items-center justify-between gap-3 p-4"
            >
              <span className="min-w-0">
                <strong className="block truncate">
                  {game.winner ? `${game.winner.name} won` : "Draw game"}
                </strong>
                <span className="text-base-content/60 text-sm">
                  {formatDate(game.played_at)} · {game.players} players
                </span>
              </span>
              <ResultBadge result={game.result} />
            </Link>
          ))}
        </div>
      </section>
    </div>
  )
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <h3 className="mb-4 font-bold">{title}</h3>
      {children}
    </div>
  )
}

function Empty({ children }: { children: ReactNode }) {
  return <p className="text-base-content/50 text-sm">{children}</p>
}

/** One linked row per record: name, W–L–D, and win rate. */
function RecordLinks({
  rows,
  link,
}: {
  rows: NamedRecordRow[]
  link: (row: NamedRecordRow) => Pick<LinkProps, "to" | "params">
}) {
  if (!rows.length) return <Empty>Nothing recorded yet.</Empty>
  return (
    <ul className="space-y-2">
      {rows.map((row) => (
        <li key={row.id}>
          <Link
            {...link(row)}
            className="group hover:border-primary/40 border-base-300 relative flex items-center justify-between gap-3 overflow-hidden rounded-lg border px-3 py-2 transition-colors"
          >
            <CardArtBackground imageUrl={row.art_crop_url} interactive />
            <span className="relative z-10 min-w-0">
              <span className="block truncate font-medium">{row.name}</span>
              {row.commander_name && row.commander_name !== row.name && (
                <span className="text-base-content/70 block truncate text-xs">
                  {row.commander_name}
                </span>
              )}
            </span>
            <span className="relative z-10 shrink-0 text-right text-sm tabular-nums">
              <strong className="text-primary">{row.win_rate}%</strong>
              <span className="text-base-content/70 block text-xs">
                {row.wins}–{row.losses}–{row.draws}
              </span>
            </span>
          </Link>
        </li>
      ))}
    </ul>
  )
}

function ResultBadge({ result }: { result: "win" | "loss" | "draw" | null }) {
  if (!result) return null
  const styles = {
    win: "badge-success",
    loss: "badge-outline",
    draw: "badge-warning",
  } as const
  return <span className={`badge ${styles[result]} shrink-0 uppercase`}>{result}</span>
}
