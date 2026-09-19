import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { UserPlus, Users } from "lucide-react"
import { getPlayers } from "@/lib/games"

export const Route = createFileRoute("/players/")({ component: PlayersPage })

function PlayersPage() {
  const query = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  return (
    <div className="flex flex-col gap-6">
      <div>
        <p className="text-primary text-sm font-semibold uppercase">The regulars</p>
        <h1 className="text-3xl font-bold">Players</h1>
      </div>
      {query.isPending && <span className="loading loading-spinner" />}
      {query.data?.length === 0 && (
        <div className="hero border-base-300 bg-base-200 rounded-box border py-16 text-center">
          <div className="hero-content flex-col">
            <Users className="text-primary size-10" />
            <h2 className="text-2xl font-bold">No players yet</h2>
            <p className="text-base-content/70">Players are created inline when you log a game.</p>
            <Link to="/games/new" className="btn btn-primary">
              <UserPlus className="size-4" /> Log a game
            </Link>
          </div>
        </div>
      )}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {query.data?.map((player) => (
          <Link
            key={player.id}
            to="/players/$playerId"
            params={{ playerId: String(player.id) }}
            className="card border-base-300 bg-base-200 hover:border-primary border"
          >
            <div className="card-body p-5">
              <div className="avatar placeholder">
                <div className="bg-primary text-primary-content w-12 rounded-full">
                  <span className="text-lg">{player.name.slice(0, 2).toUpperCase()}</span>
                </div>
              </div>
              <h2 className="mt-2 text-xl font-bold">{player.name}</h2>
              <p className="text-base-content/60 text-sm">View decks and recent games</p>
            </div>
          </Link>
        ))}
      </div>
    </div>
  )
}
