import { useQuery } from "@tanstack/react-query"
import { ChevronDown, ChevronRight, RefreshCw } from "lucide-react"
import { useState } from "react"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import { formValue } from "@/lib/form"
import { AuditChanges } from "./audit-changes"
import { AuditPagination, type AuditPage } from "./audit-pagination"

interface Operation {
  id: number
  actor_id: number | null
  actor_name: string | null
  action: string
  target: string
  request_id: string | null
  status: number | null
  inserted_at: string
  completed_at: string | null
  change_count: number
}

function Outcome({ status }: { status: number | null }) {
  if (status === null)
    return <span className="badge badge-ghost badge-sm">Unknown / in progress</span>
  return (
    <span className={`badge badge-sm ${status >= 400 ? "badge-error" : "badge-success"}`}>
      {status >= 400 ? "Failed" : status === 202 ? "Accepted" : "Succeeded"} · {status}
    </span>
  )
}

function OperationRow({ operation }: { operation: Operation }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <article className="card border-base-300 bg-base-200 min-w-0 border">
      <button
        type="button"
        aria-expanded={expanded}
        aria-controls={`audit-${operation.id}`}
        onClick={() => setExpanded(!expanded)}
        className="hover:bg-base-300/30 flex w-full items-start gap-3 rounded-[inherit] p-4 text-left"
      >
        {expanded ? (
          <ChevronDown className="mt-1 size-4 shrink-0" />
        ) : (
          <ChevronRight className="mt-1 size-4 shrink-0" />
        )}
        <span className="flex min-w-0 flex-1 flex-col gap-2">
          <span className="flex flex-wrap items-center gap-2">
            <span className="font-semibold">
              {operation.actor_name ? `@${operation.actor_name}` : "Unauthenticated"}
            </span>
            <Outcome status={operation.status} />
            <time
              dateTime={operation.inserted_at}
              className="text-base-content/60 text-xs sm:ml-auto"
            >
              {new Date(operation.inserted_at).toLocaleString()}
            </time>
          </span>
          <span className="font-mono text-sm break-all">{operation.action}</span>
          <span className="text-base-content/65 text-xs break-all">
            {operation.target} · {operation.change_count} row{" "}
            {operation.change_count === 1 ? "change" : "changes"}
          </span>
        </span>
      </button>
      {expanded && (
        <div
          id={`audit-${operation.id}`}
          className="border-base-300 flex flex-col gap-4 border-t p-4"
        >
          <p className="text-base-content/60 text-xs break-all">
            Operation #{operation.id} · Actor ID: {operation.actor_id ?? "not identified"} ·
            Request: {operation.request_id ?? "not applicable"}
          </p>
          <AuditChanges operationId={operation.id} />
        </div>
      )}
    </article>
  )
}

export function AuditLogPage() {
  const [filters, setFilters] = useState({ search: "", outcome: "", page: 1 })
  const history = useQuery({
    queryKey: ["admin", "audit", filters],
    queryFn: () =>
      api<AuditPage<Operation>>(
        `/api/admin/audit?${new URLSearchParams({ search: filters.search, outcome: filters.outcome, page: String(filters.page) })}`,
      ),
    gcTime: 0,
  })

  return (
    <div className="flex min-w-0 flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Audit log"
        description="User operations and committed before-and-after changes. History starts when auditing is enabled; credentials and request bodies are never stored."
        actions={
          <button
            type="button"
            className="btn btn-sm"
            onClick={() => void history.refetch()}
            disabled={history.isFetching}
          >
            <RefreshCw className="size-4" />
            Refresh
          </button>
        }
      />
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          setFilters({
            search: formValue(event.currentTarget, "search"),
            outcome: formValue(event.currentTarget, "outcome"),
            page: 1,
          })
        }}
      >
        <label className="flex min-w-0 basis-full flex-col gap-1 text-sm sm:flex-1 sm:basis-0">
          Search user, operation, or target
          <input
            name="search"
            type="search"
            className="input w-full"
            placeholder="e.g. /api/games or username"
          />
        </label>
        <label className="flex min-w-0 flex-1 flex-col gap-1 text-sm sm:flex-none">
          Outcome
          <select name="outcome" className="select w-full">
            <option value="">All outcomes</option>
            <option value="success">Succeeded / accepted</option>
            <option value="failed">Failed</option>
            <option value="unknown">Unknown / in progress</option>
          </select>
        </label>
        <button type="submit" className="btn">
          Apply filters
        </button>
      </form>
      {history.isPending && <p role="status">Loading audit history…</p>}
      {isSudoRequired(history.error) ? (
        <SudoPrompt error={history.error} onSuccess={() => void history.refetch()} />
      ) : history.error ? (
        <p role="alert" className="alert alert-error">
          {errorMessage(history.error)}
        </p>
      ) : (
        history.data && (
          <>
            {history.data.data.length === 0 && (
              <p className="border-base-300 text-base-content/65 rounded-box border border-dashed p-6">
                No operations match these filters.
              </p>
            )}
            <div className="flex flex-col gap-3">
              {history.data.data.map((operation) => (
                <OperationRow key={operation.id} operation={operation} />
              ))}
            </div>
            <AuditPagination
              pagination={history.data.pagination}
              onPage={(page) => setFilters({ ...filters, page })}
            />
          </>
        )
      )}
      <p className="text-base-content/55 text-xs">
        Snapshots cover supported database records, including related rows and deletions. A failed
        operation may still have committed changes; inspect its details. Unknown means unfinished or
        an unrecorded outcome. This is not an automatic undo or a replacement for backups.
      </p>
    </div>
  )
}
