import { createFileRoute } from "@tanstack/react-router"
import { ImportPage } from "@/features/imports/import-page"
import { requireAdmin } from "@/lib/auth"

export const Route = createFileRoute("/import")({
  beforeLoad: ({ context, location }) => requireAdmin(context.queryClient, location.href),
  component: ImportPage,
})
