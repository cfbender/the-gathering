import { Navigate, createFileRoute } from "@tanstack/react-router"
import { TabletopBoard } from "@/features/tabletop/tabletop-board"
import { useTabletopGame } from "@/features/tabletop/tabletop-store"

export const Route = createFileRoute("/tabletop/play")({
  component: TabletopPlayPage,
})

function TabletopPlayPage() {
  const [game] = useTabletopGame()
  if (!game) return <Navigate to="/tabletop" replace />
  return <TabletopBoard game={game} />
}
