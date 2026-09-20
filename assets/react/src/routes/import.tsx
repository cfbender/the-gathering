import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { CheckCircle2, Download, FileSpreadsheet, Upload, XCircle } from "lucide-react"
import { useState } from "react"
import type { ChangeEvent } from "react"
import { errorMessage, requireAdmin } from "@/lib/auth"
import { importCSV, previewCSV } from "@/lib/imports"

export const Route = createFileRoute("/import")({
  beforeLoad: ({ context, location }) => requireAdmin(context.queryClient, location.href),
  component: ImportPage,
})

function ImportPage() {
  const queryClient = useQueryClient()
  const [csv, setCSV] = useState("")
  const preview = useMutation({ mutationFn: previewCSV })
  const commit = useMutation({
    mutationFn: importCSV,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["games"] })
      void queryClient.invalidateQueries({ queryKey: ["players"] })
      void queryClient.invalidateQueries({ queryKey: ["decks"] })
    },
  })

  function updateCSV(value: string) {
    setCSV(value)
    preview.reset()
    commit.reset()
  }

  async function chooseFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0]
    if (file) updateCSV(await file.text())
  }

  const data = preview.data

  return (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Import game history"
        description="Upload The Gathering’s template or paste CSV exported from the Mythic Track import spreadsheet. Nothing is saved until you confirm."
        actions={
          <a className="btn btn-outline btn-sm" href="/api/imports/csv/sample" download>
            <Download className="size-4" /> Sample CSV
          </a>
        }
      />

      <section className="card border-base-300 bg-base-200 border">
        <div className="card-body gap-4 p-4 sm:p-6">
          <label className="border-base-300 bg-base-100 hover:border-primary flex cursor-pointer flex-col items-center gap-2 rounded-xl border border-dashed p-6 text-center transition-colors">
            <FileSpreadsheet className="text-primary size-8" />
            <span className="font-semibold">Choose a CSV file</span>
            <span className="text-base-content/60 text-sm">or paste its contents below</span>
            <input type="file" accept=".csv,text/csv" className="sr-only" onChange={chooseFile} />
          </label>
          <label className="fieldset">
            <span className="fieldset-legend">CSV contents</span>
            <textarea
              className="textarea textarea-bordered min-h-40 w-full font-mono text-xs"
              value={csv}
              onChange={(event) => updateCSV(event.target.value)}
              placeholder="game_id,date,player,deck,commander,seat,result…"
            />
          </label>
          {(preview.error || commit.error) && (
            <div className="alert alert-error text-sm">
              {errorMessage(preview.error ?? commit.error) ?? "Import failed."}
            </div>
          )}
          <div className="card-actions justify-end">
            <button
              type="button"
              className="btn btn-primary"
              disabled={!csv.trim() || preview.isPending}
              onClick={() => preview.mutate(csv)}
            >
              {preview.isPending ? (
                <span className="loading loading-spinner loading-sm" />
              ) : (
                <Upload className="size-4" />
              )}
              Preview import
            </button>
          </div>
        </div>
      </section>

      {data && <Preview preview={data} />}

      {data?.valid && !commit.data && (
        <div className="flex flex-col items-stretch justify-between gap-3 sm:flex-row sm:items-center">
          <p className="text-base-content/70 text-sm">
            This creates {data.players.create.length} players, {data.decks.create.length} decks, and
            up to {data.games.length} games in one transaction.
          </p>
          <button
            type="button"
            className="btn btn-success"
            disabled={commit.isPending}
            onClick={() => commit.mutate(csv)}
          >
            {commit.isPending && <span className="loading loading-spinner loading-sm" />}
            Confirm import
          </button>
        </div>
      )}

      {commit.data && (
        <div className="alert alert-success items-start">
          <CheckCircle2 className="size-5" />
          <div>
            <h2 className="font-bold">Import complete</h2>
            <p>
              Created {commit.data.created} and skipped {commit.data.skipped} already-imported{" "}
              {commit.data.skipped === 1 ? "game" : "games"}.
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              {commit.data.game_ids.map((id) => (
                <Link
                  key={id}
                  to="/games/$gameId"
                  params={{ gameId: String(id) }}
                  className="link font-semibold"
                >
                  Game #{id}
                </Link>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function Preview({ preview }: { preview: Awaited<ReturnType<typeof previewCSV>> }) {
  const errorLines = new Set(preview.errors.map((error) => error.line))

  return (
    <section className="flex min-w-0 flex-col gap-4" aria-labelledby="preview-heading">
      <div className={`alert ${preview.valid ? "alert-success" : "alert-error"}`}>
        {preview.valid ? <CheckCircle2 className="size-5" /> : <XCircle className="size-5" />}
        <div>
          <h2 id="preview-heading" className="font-bold">
            {preview.valid ? `${preview.games.length} games ready` : "Fix errors before importing"}
          </h2>
          <p className="text-sm">
            {preview.players.matched.length} players and {preview.decks.matched.length} decks match
            existing records.
          </p>
        </div>
      </div>

      {preview.errors.length > 0 && (
        <ul className="grid gap-2" aria-label="CSV errors">
          {preview.errors.map((error, index) => (
            <li
              key={`${error.line}-${error.field}-${index}`}
              className="border-error/40 bg-error/10 rounded-lg border px-4 py-3 text-sm"
            >
              <strong>Line {error.line}</strong> · {error.field}: {error.message}
            </li>
          ))}
        </ul>
      )}

      {preview.games.length > 0 && (
        <div className="rounded-box border-base-300 w-full max-w-full overflow-x-auto border">
          <table className="table bg-base-100 min-w-180">
            <thead>
              <tr>
                <th>Game</th>
                <th>Date</th>
                <th>Seat</th>
                <th>Player</th>
                <th>Deck / commander</th>
                <th>Result</th>
              </tr>
            </thead>
            <tbody>
              {preview.games.flatMap((game) =>
                game.seats.map((seat) => (
                  <tr
                    key={`${game.game_id}-${seat.line}-${seat.seat}`}
                    className={errorLines.has(seat.line) ? "bg-error/10" : undefined}
                  >
                    <td className="font-mono text-xs">{game.game_id}</td>
                    <td>{new Date(game.played_at).toLocaleDateString()}</td>
                    <td>{seat.seat}</td>
                    <td className="font-semibold">{seat.player}</td>
                    <td>
                      {seat.deck}
                      <span className="text-base-content/60 block text-xs">{seat.commander}</span>
                    </td>
                    <td>
                      <span
                        className={`badge ${seat.result === "win" ? "badge-success" : seat.result === "draw" ? "badge-info" : "badge-ghost"}`}
                      >
                        {seat.result}
                      </span>
                    </td>
                  </tr>
                )),
              )}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
