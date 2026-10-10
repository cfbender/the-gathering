import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import { AuditPagination, type AuditPage } from "./audit-pagination"

interface Change {
  id: number
  entity: string
  entity_id: string
  before: Record<string, unknown> | null
  after: Record<string, unknown> | null
}

function ChangeRow({ change }: { change: Change }) {
  const fields = [
    ...new Set([...Object.keys(change.before ?? {}), ...Object.keys(change.after ?? {})]),
  ]
  const changed = fields.filter(
    (field) => JSON.stringify(change.before?.[field]) !== JSON.stringify(change.after?.[field]),
  )
  return (
    <section
      className="flex min-w-0 flex-col gap-3"
      aria-label={`${change.entity} ${change.entity_id}`}
    >
      <h3 className="flex flex-wrap items-center gap-2 text-sm font-semibold">
        {change.entity} #{change.entity_id}
        <span className="badge badge-outline badge-sm">
          {change.before === null ? "Created" : change.after === null ? "Deleted" : "Updated"}
        </span>
        <span className="text-base-content/50 font-normal">Change #{change.id}</span>
      </h3>
      <p className="text-base-content/65 text-xs break-words">
        Changed fields: {changed.join(", ") || "none"}
      </p>
      <div className="grid min-w-0 gap-3 lg:grid-cols-2">
        {(["before", "after"] as const).map((side) => (
          <div key={side} className="min-w-0">
            <h4 className="mb-1 text-xs font-semibold uppercase">{side}</h4>
            <pre className="bg-base-300/50 rounded-box max-h-96 overflow-auto p-3 font-mono text-xs break-all whitespace-pre-wrap">
              {change[side] === null
                ? "Record does not exist"
                : JSON.stringify(change[side], null, 2)}
            </pre>
          </div>
        ))}
      </div>
    </section>
  )
}

export function AuditChanges({ operationId }: { operationId: number }) {
  const [page, setPage] = useState(1)
  const changes = useQuery({
    queryKey: ["admin", "audit", operationId, "changes", page],
    queryFn: () => api<AuditPage<Change>>(`/api/admin/audit/${operationId}?page=${page}`),
    gcTime: 0,
  })
  if (changes.isPending) return <p role="status">Loading changes…</p>
  if (isSudoRequired(changes.error))
    return <SudoPrompt error={changes.error} onSuccess={() => void changes.refetch()} />
  if (changes.error)
    return (
      <p role="alert" className="text-error">
        {errorMessage(changes.error)}
      </p>
    )
  if (!changes.data) return null
  if (changes.data.data.length === 0)
    return (
      <p className="text-base-content/65 text-sm">
        No row snapshots for this operation. It may have made no supported database changes, changed
        credentials only, or affected transient state.
      </p>
    )
  return (
    <div className="flex min-w-0 flex-col gap-5">
      {changes.data.data.map((change) => (
        <ChangeRow key={change.id} change={change} />
      ))}
      <AuditPagination pagination={changes.data.pagination} onPage={setPage} />
    </div>
  )
}
