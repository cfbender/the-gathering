import { createFileRoute } from "@tanstack/react-router"
import { AdminDiscordPage } from "@/features/admin/admin-pages"

export const Route = createFileRoute("/admin/discord")({ component: AdminDiscordPage })
