import { createFileRoute } from "@tanstack/react-router"
import { TabletopSetup } from "@/features/tabletop/tabletop-setup"

export const Route = createFileRoute("/tabletop/")({
  component: TabletopSetup,
})
