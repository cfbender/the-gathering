import { createFileRoute } from "@tanstack/react-router"
import { AdminCatalogPage } from "@/features/admin/admin-pages"

export const Route = createFileRoute("/admin/catalog")({ component: AdminCatalogPage })
