import { createFileRoute } from "@tanstack/react-router"
import { Activity, BarChart3, Bot, FileSpreadsheet, Library, Users } from "lucide-react"
import { useQuery } from "@tanstack/react-query"
import { api } from "@/lib/api"
import { cn } from "@/lib/cn"

export const Route = createFileRoute("/")({
  component: HomePage,
})

interface Health {
  status: "ok" | "error"
}

const planned = [
  {
    Icon: Users,
    title: "Playgroup accounts",
    body: "Everyone gets a login; the admin runs the server.",
  },
  {
    Icon: FileSpreadsheet,
    title: "CSV import",
    body: "Bring your existing game history with you.",
  },
  {
    Icon: Library,
    title: "Scryfall catalog",
    body: "Commander and MVP search backed by a local card index.",
  },
  { Icon: Bot, title: "Discord tracking", body: "Games logged automatically from SpellBot." },
  { Icon: BarChart3, title: "Stats", body: "Win rates, matchups, and deck performance over time." },
]

function HomePage() {
  const query = useQuery({
    queryKey: ["health"],
    queryFn: () => api<Health>("/api/health"),
    refetchInterval: 30_000,
  })
  const health =
    query.status === "pending"
      ? { kind: "loading" as const }
      : query.status === "error"
        ? { kind: "error" as const, message: query.error.message }
        : query.data.status === "ok"
          ? { kind: "ok" as const }
          : { kind: "error" as const, message: "degraded" }

  return (
    <div className="flex flex-col gap-10">
      <section className="flex flex-col gap-4">
        <p className="text-primary text-sm font-semibold tracking-wide uppercase">
          Commander game tracker
        </p>
        <h1 className="max-w-2xl text-4xl font-bold tracking-tight text-balance sm:text-5xl">
          Every game your playgroup has ever played, in one place.
        </h1>
        <p className="text-base-content/70 max-w-xl text-lg">
          Self-hosted, one container, your data. Log games by hand, import a spreadsheet, or let
          Discord do it for you.
        </p>
      </section>

      <section aria-labelledby="status-heading" className="card bg-base-200 border-base-300 border">
        <div className="card-body flex-row items-center gap-4">
          <span
            className={cn(
              "grid size-10 shrink-0 place-items-center rounded-full",
              health.kind === "ok" && "bg-success/15 text-success",
              health.kind === "error" && "bg-error/15 text-error",
              health.kind === "loading" && "bg-base-300 text-base-content/60",
            )}
          >
            <Activity className="size-5" aria-hidden="true" />
          </span>
          <div>
            <h2 id="status-heading" className="font-semibold">
              Server status
            </h2>
            <p className="text-base-content/70 text-sm" data-testid="health-status">
              {health.kind === "loading" && "Checking the API…"}
              {health.kind === "ok" && "API and database are reachable."}
              {health.kind === "error" && `API unreachable: ${health.message}`}
            </p>
          </div>
        </div>
      </section>

      <section aria-labelledby="planned-heading" className="flex flex-col gap-4">
        <h2 id="planned-heading" className="text-base-content/60 text-sm font-semibold uppercase">
          On the roadmap
        </h2>
        <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {planned.map(({ Icon, title, body }) => (
            <li key={title} className="card bg-base-200 border-base-300 border">
              <div className="card-body gap-2">
                <Icon className="text-accent size-5" aria-hidden="true" />
                <h3 className="font-semibold">{title}</h3>
                <p className="text-base-content/70 text-sm">{body}</p>
              </div>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}
