import { createFileRoute } from "@tanstack/react-router"
import { PlaygroupsPage } from "@/features/admin/playgroups-page"

export const Route = createFileRoute("/admin/playgroups")({ component: PlaygroupsPage })
