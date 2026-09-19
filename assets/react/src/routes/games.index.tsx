import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { CalendarDays, Plus, Trophy } from "lucide-react"
import { useState } from "react"
import { formatDate, getGames, getPlayers } from "@/lib/games"

export const Route = createFileRoute("/games/")({ component: GamesPage })

function GamesPage() {
  const [playerId, setPlayerId] = useState("")
  const [dateFrom, setDateFrom] = useState("")
  const [dateTo, setDateTo] = useState("")
  const [page, setPage] = useState(1)
  const filters = { player_id: playerId, date_from: dateFrom, date_to: dateTo, page }
  const games = useQuery({ queryKey: ["games", filters], queryFn: () => getGames(filters) })
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <p className="text-primary text-sm font-semibold uppercase">Playgroup history</p>
          <h1 className="text-3xl font-bold tracking-tight">Games</h1>
        </div>
        <Link to="/games/new" className="btn btn-primary">
          <Plus className="size-4" /> Log game
        </Link>
      </div>

      <section aria-label="Game filters" className="card border-base-300 bg-base-200 border">
        <div className="card-body grid gap-3 p-4 sm:grid-cols-3">
          <label className="form-control">
            <span className="label-text mb-1 text-xs font-semibold">Player</span>
            <select
              className="select select-bordered select-sm w-full"
              value={playerId}
              onChange={(event) => {
                setPlayerId(event.target.value)
                setPage(1)
              }}
            >
              <option value="">Everyone</option>
              {players.data?.map((player) => (
                <option key={player.id} value={player.id}>
                  {player.name}
                </option>
              ))}
            </select>
          </label>
          <label className="form-control">
            <span className="label-text mb-1 text-xs font-semibold">From</span>
            <input
              type="date"
              className="input input-bordered input-sm w-full"
              value={dateFrom}
              onChange={(event) => {
                setDateFrom(event.target.value)
                setPage(1)
              }}
            />
          </label>
          <label className="form-control">
            <span className="label-text mb-1 text-xs font-semibold">Through</span>
            <input
              type="date"
              className="input input-bordered input-sm w-full"
              value={dateTo}
              onChange={(event) => {
                setDateTo(event.target.value)
                setPage(1)
              }}
            />
          </label>
        </div>
      </section>

      {games.isPending && (
        <div className="flex justify-center py-16">
          <span className="loading loading-spinner loading-lg" />
        </div>
      )}
      {games.isError && <div className="alert alert-error">Could not load games.</div>}
      {games.data?.data.length === 0 && (
        <div className="hero border-base-300 bg-base-200 rounded-box border py-16 text-center">
          <div className="hero-content flex-col">
            <CalendarDays className="text-primary size-10" />
            <h2 className="text-2xl font-bold">No games at this table yet</h2>
            <p className="text-base-content/70">
              Log the first game, or clear your filters to see more history.
            </p>
            <Link to="/games/new" className="btn btn-primary mt-2">
              Log the first game
            </Link>
          </div>
        </div>
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
                      className={`flex items-center gap-2 rounded-lg px-3 py-2 ${seat.result === "win" ? "bg-success/15 text-success" : "bg-base-100"}`}
                    >
                      {seat.result === "win" && <Trophy className="size-4 shrink-0" />}
                      <span className="min-w-0">
                        <strong className="block truncate">{seat.player.name}</strong>
                        <span className="text-base-content/60 block truncate text-xs">
                          {seat.deck?.commander_name ?? "Unknown commander"}
                        </span>
                      </span>
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
