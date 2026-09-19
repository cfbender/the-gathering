import { useQuery } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { GameForm } from "@/components/game-form"
import { getGame } from "@/lib/games"

export const Route = createFileRoute("/games/$gameId_/edit")({ component: EditGamePage })

function EditGamePage() {
  const { gameId } = Route.useParams()
  const query = useQuery({ queryKey: ["games", gameId], queryFn: () => getGame(gameId) })
  if (query.isPending)
    return (
      <div className="flex justify-center py-16">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  if (query.isError) return <div className="alert alert-error">Game not found.</div>
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6">
      <div>
        <p className="text-primary text-sm font-semibold uppercase">Game #{gameId}</p>
        <h1 className="text-3xl font-bold">Edit game</h1>
      </div>
      <GameForm game={query.data} />
    </div>
  )
}
