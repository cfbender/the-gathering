import { Link } from "@tanstack/react-router"
import { useState } from "react"
import type { CSVImportReview } from "./imports"

export function materialChanges(review: CSVImportReview) {
  return review.changes.filter((change) => change.before !== change.after)
}

export function CSVCorrectionReview({ review }: { review: CSVImportReview[] }) {
  const [showAll, setShowAll] = useState(false)
  const counts = { create: 0, update: 0, skip: 0 }
  for (const item of review) counts[item.action] += 1
  const rows = review.filter(
    (item) => showAll || item.action !== "skip" || materialChanges(item).length > 0,
  )

  return (
    <section className="card border-base-300 bg-base-200 border" aria-labelledby="review-heading">
      <div className="card-body gap-3 p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 id="review-heading" className="font-bold">
              CSV correction review
            </h3>
            <p className="text-base-content/70 text-sm">
              {counts.create} create · {counts.update} update · {counts.skip} skip
            </p>
          </div>
          <label className="label cursor-pointer gap-2 text-sm">
            <input
              type="checkbox"
              className="toggle toggle-sm"
              checked={showAll}
              onChange={(event) => setShowAll(event.target.checked)}
            />
            Show all, including unchanged
          </label>
        </div>
        <p className="text-base-content/60 text-xs">
          An explicit action=update plus source/external_id or portable_id identifies an existing
          game; dates are never used to guess. Blank optional values preserve existing data.
        </p>
        {rows.length === 0 ? (
          <p className="text-base-content/60 text-sm">No changed games.</p>
        ) : (
          <div className="grid gap-2">
            {rows.map((item) => {
              const changes = materialChanges(item)
              return (
                <details
                  key={item.game_id}
                  className="collapse-arrow bg-base-100 collapse border-base-300 border"
                >
                  <summary className="collapse-title flex flex-wrap items-center gap-2 py-3">
                    <span
                      className={`badge ${item.action === "update" ? "badge-warning" : item.action === "create" ? "badge-success" : "badge-ghost"}`}
                    >
                      {item.action}
                    </span>
                    <span className="min-w-0 break-all font-mono text-xs">{item.game_id}</span>
                    {item.target_id !== null && item.action !== "create" && (
                      <Link
                        to="/games/$gameId"
                        params={{ gameId: String(item.target_id) }}
                        className="link ml-auto text-sm"
                        onClick={(event) => event.stopPropagation()}
                      >
                        Game #{item.target_id}
                      </Link>
                    )}
                  </summary>
                  <div className="collapse-content overflow-x-auto">
                    {changes.length === 0 ? (
                      <p className="text-sm">
                        {item.action === "create"
                          ? "New game. Review its seats in the preview above."
                          : "No changes."}
                      </p>
                    ) : (
                      <table className="table table-sm block sm:table">
                        <thead className="hidden sm:table-header-group">
                          <tr>
                            <th>Field</th>
                            <th>Player</th>
                            <th>Before</th>
                            <th>After</th>
                          </tr>
                        </thead>
                        <tbody className="block sm:table-row-group">
                          {changes.map((change, index) => (
                            <tr
                              key={`${change.field}-${change.player}-${index}`}
                              className="grid grid-cols-2 py-2 sm:table-row"
                            >
                              <td className="min-w-0 wrap-anywhere font-medium">{change.field}</td>
                              <td className="min-w-0 wrap-anywhere">{change.player ?? "—"}</td>
                              <td className="min-w-0 wrap-anywhere">
                                <span className="text-base-content/60 block text-xs sm:hidden">
                                  Before
                                </span>
                                {display(change.before)}
                              </td>
                              <td className="min-w-0 wrap-anywhere">
                                <span className="text-base-content/60 block text-xs sm:hidden">
                                  After
                                </span>
                                {display(change.after)}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )}
                  </div>
                </details>
              )
            })}
          </div>
        )}
      </div>
    </section>
  )
}

function display(value: string | number | null) {
  return value === null || value === "" ? "—" : String(value)
}
