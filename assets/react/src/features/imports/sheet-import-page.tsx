import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { invalidateGameRelated } from "@/features/games/games"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import {
  changeGroup,
  commitSheet,
  emptySheetInput,
  previewSheet,
  type SheetInput,
} from "./sheet-import"
import { SheetRowReview } from "./sheet-row-review"

const filters = [
  ["changes", "All changes"],
  ["corrections", "Results or decks"],
  ["details", "Kills or notes only"],
  ["review", "Needs review"],
  ["unchanged", "Unchanged"],
  ["reconciled", "Already reconciled"],
  ["all", "All rows"],
] as const

export function SheetImportPage() {
  const queryClient = useQueryClient()
  const [input, setInput] = useState(emptySheetInput)
  const [filter, setFilter] = useState("changes")
  const preview = useMutation({ mutationFn: previewSheet })
  const commit = useMutation({
    mutationFn: commitSheet,
    onSuccess: () => {
      void invalidateGameRelated(queryClient)
    },
  })
  const data = preview.data
  const dirty = JSON.stringify(input) !== JSON.stringify(preview.variables)
  const busy = preview.isPending || commit.isPending
  const names = [
    ...new Set(
      data?.rows.flatMap((row) => [
        ...row.seats.map((seat) => seat.player),
        ...row.kill_counts.map((count) => count.player),
      ]) ?? [],
    ),
  ].sort()
  const selected = data?.rows.filter((row) => row.action !== "skip") ?? []
  const rowsFor = (value: string) =>
    data?.rows.filter(
      (row) =>
        value === "all" ||
        (value === "changes" ? row.status === "changed" : changeGroup(row) === value),
    ) ?? []

  function change(next: SheetInput) {
    setInput(next)
    commit.reset()
  }

  function changeText(text: string) {
    change({ ...emptySheetInput(), text })
    preview.reset()
  }

  const error = preview.error ?? (isSudoRequired(commit.error) ? null : commit.error)

  return (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Reconcile Google Sheet"
        description="Update selected games without deleting history. Paste cells or upload the original one-game-per-row CSV."
      />
      <p className="text-base-content/70 text-sm">
        Updates keep game IDs, timestamps, turn order, duration, MVPs and cleaned-up decks unless
        you choose a deck mapping. Review the actual changes below before confirming. Blank sheet
        kills mean zero and replace existing counts. Empty notes keep existing notes.
      </p>
      <fieldset disabled={busy} className="flex min-w-0 flex-col gap-4">
        <label className="fieldset">
          <span className="fieldset-legend">Google Sheet CSV or TSV file</span>
          <input
            type="file"
            accept=".csv,.tsv,text/csv,text/tab-separated-values"
            className="file-input w-full"
            onChange={async (event) => {
              const file = event.target.files?.[0]
              if (file) changeText(await file.text())
            }}
          />
        </label>
        <label className="fieldset">
          <span className="fieldset-legend">Sheet contents</span>
          <textarea
            className="textarea min-h-32 w-full font-mono text-xs"
            value={input.text}
            placeholder="Date → Winner → Deck → player kill columns → Win Con → Other Decks → Notes"
            onChange={(event) => changeText(event.target.value)}
          />
        </label>
        <button
          className="btn btn-primary self-end"
          type="button"
          disabled={!input.text.trim()}
          onClick={() => preview.mutate(input)}
        >
          {busy ? "Working…" : data ? "Refresh preview" : "Read sheet"}
        </button>
        {data && (
          <>
            <details className="card border-base-300 bg-base-200 border p-4">
              <summary className="cursor-pointer font-bold">
                Player mappings ({names.length} sheet names)
              </summary>
              <p className="text-base-content/70 my-2 text-sm">
                Map aliases such as Dan / Daniel or Ryan / Reality explicitly. A mapping also
                applies to that player's kill column. Refresh to see updated deck choices and
                validation.
              </p>
              <div className="grid gap-3 sm:grid-cols-3">
                {names.map((name) => (
                  <label key={name} className="flex flex-col gap-1 text-sm">
                    {name}
                    <select
                      className="select w-full"
                      value={input.players[name] ?? ""}
                      onChange={(event) => {
                        const players = { ...input.players }
                        if (event.target.value === "") delete players[name]
                        else
                          players[name] =
                            event.target.value === "new" ? "new" : Number(event.target.value)
                        change({ ...input, players })
                      }}
                    >
                      <option value="">Exact-name match only</option>
                      <option value="new">Create {name}</option>
                      {data.players.map((player) => (
                        <option key={player.id} value={player.id}>
                          {player.name}
                        </option>
                      ))}
                    </select>
                  </label>
                ))}
              </div>
            </details>
            <p role="status" className="text-sm">
              {data.rows.length} rows · {rowsFor("corrections").length} result/deck corrections ·{" "}
              {rowsFor("details").length} kills/notes only · {rowsFor("review").length} need review
              · {rowsFor("unchanged").length} unchanged · {rowsFor("reconciled").length} already
              reconciled.
            </p>
            <p className="text-base-content/70 text-sm">
              Unambiguous matches with changes are selected automatically. Unchanged, unresolved and
              previously reconciled rows are skipped. The confirmation applies all selected rows,
              including those hidden by this filter. Nothing is saved until you confirm.
            </p>
            <label className="flex flex-wrap items-center gap-3 text-sm">
              Show rows
              <select
                className="select w-full sm:w-auto"
                value={filter}
                onChange={(event) => setFilter(event.target.value)}
              >
                {filters.map(([value, label]) => (
                  <option key={value} value={value}>
                    {label} ({rowsFor(value).length})
                  </option>
                ))}
              </select>
            </label>
            {rowsFor(filter).length === 0 && (
              <p className="text-base-content/70 text-sm">No rows in this category.</p>
            )}
            {rowsFor(filter).map((row) => (
              <SheetRowReview
                key={row.key}
                row={row}
                input={input}
                preview={data}
                dirty={dirty}
                onChange={change}
              />
            ))}
          </>
        )}
      </fieldset>
      {error && (
        <p role="alert" className="alert alert-error">
          {errorMessage(error, "import") ??
            errorMessage(error) ??
            "Import failed. Review your selections."}
        </p>
      )}
      {data && !commit.data && (
        <div className="bg-base-100 border-base-300 sticky bottom-2 flex flex-wrap items-center justify-between gap-3 rounded-xl border p-4 shadow-lg">
          <p className="text-sm">
            {dirty
              ? "Selections changed — refresh before confirming."
              : `${selected.filter((row) => row.action === "create").length} creates, ${selected.filter((row) => typeof row.action === "number").length} updates; ${data.rows.length - selected.length} skipped.`}
          </p>
          <div className="flex gap-2">
            <button
              className="btn btn-outline btn-sm"
              disabled={busy}
              onClick={() => preview.mutate(input)}
            >
              Refresh preview
            </button>
            <button
              className="btn btn-success btn-sm"
              disabled={busy || dirty || !data.valid}
              onClick={() => commit.mutate({ ...input, revision: data.revision })}
            >
              Confirm selected changes
            </button>
          </div>
        </div>
      )}
      <SudoPrompt
        error={commit.error}
        onSuccess={() => commit.variables && commit.mutate(commit.variables)}
      />
      {commit.data && (
        <div role="status" className="alert alert-success flex flex-col items-start">
          <p>
            Reconciled: {commit.data.updated} updated, {commit.data.created} created,{" "}
            {commit.data.skipped} skipped. Refresh to see recorded matches.
          </p>
          <div className="flex flex-wrap gap-3">
            {commit.data.game_ids.map((id) => (
              <Link key={id} className="link" to="/games/$gameId" params={{ gameId: String(id) }}>
                Game #{id}
              </Link>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
