import { createFileRoute } from "@tanstack/react-router"
import { WebcamTablePage } from "@/features/webcam-table/webcam-table-page"

export const Route = createFileRoute("/table/$roomId")({
  component: TableRoomRoute,
})

function TableRoomRoute() {
  const { roomId } = Route.useParams()
  return <WebcamTablePage roomId={roomId} />
}
