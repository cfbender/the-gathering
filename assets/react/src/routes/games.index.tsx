import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { CalendarDays, Plus, Trophy, X } from "lucide-react"
import { useEffect, useState } from "react"
import { formatDate, getGames, getPlayers } from "@/features/games/games"
import { CardArtBackground } from "@/components/card-art-background"

export const Route = createFileRoute("/games/")({ component: GamesPage })

const emptyFilters = {
  player_id: "",
  winner_id: "",
  commander: "",
  player_count: "",
  date_from: "",
  date_to: "",
  min_turns: "",
  max_turns: "",
  min_duration: "",
  max_duration: "",
}

type Filters = typeof emptyFilters

const seatCounts = ["2", "3", "4", "5", "6"]

function GamesPage() {
  const [filters, setFilters] = useState<Filters>(emptyFilters)
  const [page, setPage] = useState(1)
  // Commander is free text; wait for a pause in typing before querying.
  const [commander, setCommander] = useState("")
  useEffect(() => {
    const timer = window.setTimeout(() => {
      setFilters((current) =>
        current.commander === commander.trim()
          ? current
          : { ...current, commander: commander.trim() },
      )
      setPage(1)
    }, 250)
    return () => window.clearTimeout(timer)
  }, [commander])

  function update(patch: Partial<Filters>) {
    setFilters((current) => ({ ...current, ...patch }))
    setPage(1)
  }

  function clear() {
    setFilters(emptyFilters)
    setCommander("")
    setPage(1)
  }

  const activeCount = Object.values(filters).filter((value) => value !== "").length
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
      <div className="grid gap-4 lg:grid-cols-2">
        {games.data?.data.map((game) => {
          const winner = game.seats.find((seat) => seat.result === "win")
          return (
            <Link
              key={game.id}
              to="/games/$gameId"
              params={{ gameId: String(game.id) }}
              className="card border-base-300 bg-base-200 hover:border-primary border transition-colors"
            >
              <div className="card-body gap-4 p-5">
                <div className="flex items-center justify-between gap-3">
                  <h2 className="font-bold">{formatDate(game.played_at)}</h2>
                  <span className="badge badge-ghost">{game.seats.length} players</span>
                </div>
                <ul className="grid gap-2 sm:grid-cols-2">
                  {game.seats.map((seat) => (
                    <li
                      key={seat.id}
                      className={`relative flex items-center gap-2 overflow-hidden rounded-lg px-3 py-2 ${seat.result === "win" ? "border-success border bg-success/15" : "bg-base-100"}`}
                    >
                      <CardArtBackground imageUrl={seat.deck?.commander_art_crop_url} />
                      {seat.result === "win" && (
                        <Trophy className="text-success relative z-10 size-4 shrink-0" />
                      )}
                      <span className="text-base-content relative z-10 min-w-0 flex-1">
                        <strong className="block truncate">{seat.player.name}</strong>
                        <span className="text-base-content/85 block truncate text-xs">
                          {seat.deck?.commander_name ?? "Unknown commander"}
                        </span>
                      </span>
                      {seat.deck && <ColorIdentity colors={seat.deck.color_identity} />}
                    </li>
                  ))}
                </ul>
                {winner && (
                  <p className="text-success text-sm font-semibold">
                    {winner.player.name} won with {winner.deck?.name ?? "an unknown deck"}
                  </p>
                )}
                {!winner && <p className="text-info text-sm font-semibold">Draw</p>}
              </div>
            </Link>
          )
        })}
      </div>
      {games.data && games.data.pagination.total_pages > 1 && (
        <div className="join self-center">
          <button
            className="btn join-item"
            disabled={page <= 1}
            onClick={() => setPage((value) => value - 1)}
          >
            Previous
          </button>
          <span className="btn join-item pointer-events-none">
            {page} / {games.data.pagination.total_pages}
          </span>
          <button
            className="btn join-item"
            disabled={page >= games.data.pagination.total_pages}
            onClick={() => setPage((value) => value + 1)}
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
