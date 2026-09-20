import { createFileRoute } from "@tanstack/react-router"
import { AdminSettingsPage } from "@/features/admin/admin-pages"

export const Route = createFileRoute("/admin/settings")({ component: AdminSettingsPage })
