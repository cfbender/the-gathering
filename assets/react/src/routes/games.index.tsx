import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { CalendarDays, Plus, X } from "lucide-react"
import { useEffect, useState } from "react"
import { getGames, getPlayers } from "@/features/games/games"
import { GameCard } from "@/features/games/game-card"
import { GameTable } from "@/features/games/game-table"
import { GameViewToggle, type GameView } from "@/features/games/game-view-toggle"
import {
  countActiveGameFilters,
  parseGamesSearch,
  patchGamesSearch,
  toGameFilters,
  type GameFilters,
} from "@/features/games/game-filters"

export const Route = createFileRoute("/games/")({
  validateSearch: parseGamesSearch,
  component: GamesPage,
})

const seatCounts = ["2", "3", "4", "5", "6"]

function GamesPage() {
  // Filters live in the URL so other pages can link to a filtered list.
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const filters = toGameFilters(search)
  const page = search.page ?? 1
  const [view, setView] = useState<GameView>("cards")

  function update(patch: Partial<GameFilters>) {
    void navigate({ search: patchGamesSearch(search, patch), replace: true })
  }

  function clear() {
    void navigate({ search: {}, replace: true })
  }

  function goToPage(next: number) {
    void navigate({ search: { ...search, page: next > 1 ? next : undefined } })
  }

  // Commander is free text; wait for a pause in typing before writing it to the URL.
  const [commander, setCommander] = useState(filters.commander)
  const urlCommander = filters.commander
  useEffect(() => {
    if (commander.trim() === urlCommander) return
    const timer = window.setTimeout(() => update({ commander: commander.trim() }), 250)
    return () => window.clearTimeout(timer)
  }, [commander, urlCommander])
  // The URL changed underneath the input (back button, external link): follow it.
  const [seenUrlCommander, setSeenUrlCommander] = useState(urlCommander)
  if (urlCommander !== seenUrlCommander) {
    setSeenUrlCommander(urlCommander)
    if (commander.trim() !== urlCommander) setCommander(urlCommander)
  }

  const activeCount = countActiveGameFilters(search)
  const params = { ...filters, page }
  const games = useQuery({ queryKey: ["games", params], queryFn: () => getGames(params) })
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })

  const playerOptions = players.data?.map((player) => (
    <option key={player.id} value={player.id}>
      {player.name}
    </option>
  ))

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Playgroup history"
        title="Games"
        actions={
          <Link to="/games/new" className="btn btn-primary">
            <Plus className="size-4" /> Log game
          </Link>
        }
      />

      <section aria-label="Game filters" className="card border-base-300 bg-base-200 border">
        <div className="card-body gap-3 p-4">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <label className="form-control">
              <span className="label-text mb-1 text-xs font-semibold">Player</span>
              <select
                className="select select-bordered select-sm w-full"
                value={filters.player_id}
                onChange={(event) => update({ player_id: event.target.value })}
              >
                <option value="">Everyone</option>
                {playerOptions}
              </select>
            </label>
            <label className="form-control">
              <span className="label-text mb-1 text-xs font-semibold">Winner</span>
              <select
                className="select select-bordered select-sm w-full"
                value={filters.winner_id}
                onChange={(event) => update({ winner_id: event.target.value })}
              >
                <option value="">Anyone</option>
                {playerOptions}
              </select>
            </label>
            <label className="form-control">
              <span className="label-text mb-1 text-xs font-semibold">Commander</span>
              <input
                type="search"
                className="input input-bordered input-sm w-full"
                placeholder="Any commander or partner"
                value={commander}
                onChange={(event) => setCommander(event.target.value)}
              />
            </label>
            <label className="form-control">
              <span className="label-text mb-1 text-xs font-semibold">Pod size</span>
              <select
                className="select select-bordered select-sm w-full"
                value={filters.player_count}
                onChange={(event) => update({ player_count: event.target.value })}
              >
                <option value="">Any size</option>
                {seatCounts.map((count) => (
                  <option key={count} value={count}>
                    {count} players
                  </option>
                ))}
              </select>
            </label>
            <label className="form-control">
              <span className="label-text mb-1 text-xs font-semibold">From</span>
              <input
                type="date"
                className="input input-bordered input-sm w-full"
                value={filters.date_from}
                onChange={(event) => update({ date_from: event.target.value })}
              />
            </label>
            <label className="form-control">
              <span className="label-text mb-1 text-xs font-semibold">Through</span>
              <input
                type="date"
                className="input input-bordered input-sm w-full"
                value={filters.date_to}
                onChange={(event) => update({ date_to: event.target.value })}
              />
            </label>
            <RangeFilter
              label="Turns"
              min={filters.min_turns}
              max={filters.max_turns}
              onChange={(min, max) => update({ min_turns: min, max_turns: max })}
            />
            <RangeFilter
              label="Duration (minutes)"
              min={filters.min_duration}
              max={filters.max_duration}
              onChange={(min, max) => update({ min_duration: min, max_duration: max })}
            />
          </div>
          {activeCount > 0 && (
            <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
              <span className="text-base-content/70">
                {activeCount} {activeCount === 1 ? "filter" : "filters"} active
                {games.data &&
                  ` · ${games.data.pagination.total} ${games.data.pagination.total === 1 ? "game" : "games"}`}
              </span>
              <button type="button" className="btn btn-ghost btn-sm" onClick={clear}>
                <X className="size-4" /> Clear filters
              </button>
            </div>
          )}
        </div>
      </section>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-base-content/65 text-sm">
          {games.data &&
            `${games.data.pagination.total} ${games.data.pagination.total === 1 ? "game" : "games"}`}
        </p>
        <GameViewToggle value={view} onChange={setView} />
      </div>

      {games.isPending && (
        <div className="flex justify-center py-16">
          <span className="loading loading-spinner loading-lg" />
        </div>
      )}
      {games.isError && <div className="alert alert-error">Could not load games.</div>}
      {games.data?.data.length === 0 && (
        <EmptyPanel
          icon={<CalendarDays className="size-10" />}
          title="No games at this table yet"
          description="Log the first game, or clear your filters to see more history."
          action={
            <Link to="/games/new" className="btn btn-primary">
              Log the first game
            </Link>
          }
        />
      )}
      {games.data &&
        games.data.data.length > 0 &&
        (view === "table" ? (
          <GameTable games={games.data.data} />
        ) : (
          <div className="grid gap-4 lg:grid-cols-2">
            {games.data.data.map((game) => (
              <GameCard key={game.id} game={game} />
            ))}
          </div>
        ))}
      {games.data && games.data.pagination.total_pages > 1 && (
        <div className="join self-center">
          <button className="btn join-item" disabled={page <= 1} onClick={() => goToPage(page - 1)}>
            Previous
          </button>
          <span className="btn join-item pointer-events-none">
            {page} / {games.data.pagination.total_pages}
          </span>
          <button
            className="btn join-item"
            disabled={page >= games.data.pagination.total_pages}
            onClick={() => goToPage(page + 1)}
          >
            Next
          </button>
        </div>
      )}
    </div>
  )
}

/** Paired min/max numeric inputs that share one label. */
function RangeFilter({
  label,
  min,
  max,
  onChange,
}: {
  label: string
  min: string
  max: string
  onChange: (min: string, max: string) => void
}) {
  return (
    <fieldset className="form-control min-w-0">
      <legend className="label-text mb-1 text-xs font-semibold">{label}</legend>
      <div className="join w-full">
        <input
          type="number"
          inputMode="numeric"
          min={1}
          className="input input-bordered input-sm join-item w-full min-w-0"
          placeholder="Min"
          aria-label={`Minimum ${label.toLowerCase()}`}
          value={min}
          onChange={(event) => onChange(event.target.value, max)}
        />
        <input
          type="number"
          inputMode="numeric"
          min={1}
          className="input input-bordered input-sm join-item w-full min-w-0"
          placeholder="Max"
          aria-label={`Maximum ${label.toLowerCase()}`}
          value={max}
          onChange={(event) => onChange(min, event.target.value)}
        />
      </div>
    </fieldset>
  )
}
