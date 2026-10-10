import { createFileRoute } from "@tanstack/react-router"
import { AuditLogPage } from "@/features/admin/audit-log-page"

export const Route = createFileRoute("/admin/audit")({ component: AuditLogPage })
