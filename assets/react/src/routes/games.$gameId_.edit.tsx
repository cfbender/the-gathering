import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { GameForm } from "@/features/games/game-form"
import { canManageGame, getGame } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"

export const Route = createFileRoute("/games/$gameId_/edit")({
  component: EditGamePage,
})

function EditGamePage() {
  const { gameId } = Route.useParams()
  const query = useQuery({ queryKey: ["games", gameId], queryFn: () => getGame(gameId) })
  const viewer = useCurrentUser()
  if (query.isPending || viewer.isPending)
    return (
      <div className="flex justify-center py-16">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  if (query.isError) return <div className="alert alert-error">Game not found.</div>
  if (!canManageGame(viewer.data, query.data))
    return (
      <div className="alert alert-error">
        <span>You do not have permission to edit this game.</span>
        <Link to="/games/$gameId" params={{ gameId }} className="btn btn-sm">
          View game
        </Link>
      </div>
    )
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6">
      <PageHeader eyebrow={`Game #${gameId}`} title="Edit game" />
      <GameForm game={query.data} />
    </div>
  )
}
