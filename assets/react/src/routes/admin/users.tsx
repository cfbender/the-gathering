import { createFileRoute } from "@tanstack/react-router"
import { AdminUsersPage } from "@/features/admin/admin-pages"

export const Route = createFileRoute("/admin/users")({ component: AdminUsersPage })
