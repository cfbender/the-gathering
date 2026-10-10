import { createFileRoute } from "@tanstack/react-router"
import { ServerLogsPage } from "@/features/admin/server-logs-page"

export const Route = createFileRoute("/admin/server-logs")({ component: ServerLogsPage })
