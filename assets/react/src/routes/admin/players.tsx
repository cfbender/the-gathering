import { createFileRoute } from "@tanstack/react-router"
import { PlayerIdentitiesPage } from "@/features/admin/player-identities-page"

export const Route = createFileRoute("/admin/players")({ component: PlayerIdentitiesPage })
