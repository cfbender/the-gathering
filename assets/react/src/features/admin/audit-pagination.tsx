export interface AuditPage<T> {
  data: T[]
  pagination: { page: number; per_page: number; total: number }
}

export function AuditPagination({
  pagination,
  onPage,
}: {
  pagination: AuditPage<unknown>["pagination"]
  onPage: (page: number) => void
}) {
  const pages = Math.max(1, Math.ceil(pagination.total / pagination.per_page))
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
      <span className="text-base-content/65">
        {pagination.total} {pagination.total === 1 ? "record" : "records"} · Page {pagination.page}{" "}
        of {pages}
      </span>
      <div className="join">
        <button
          type="button"
          className="btn btn-sm join-item"
          disabled={pagination.page <= 1}
          onClick={() => onPage(pagination.page - 1)}
        >
          Previous
        </button>
        <button
          type="button"
          className="btn btn-sm join-item"
          disabled={pagination.page >= pages}
          onClick={() => onPage(pagination.page + 1)}
        >
          Next
        </button>
      </div>
    </div>
  )
}
