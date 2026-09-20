import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  FileJson,
  FileSpreadsheet,
  Upload,
  XCircle,
} from "lucide-react"
import { useState } from "react"
import type { ChangeEvent } from "react"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import { commitImport, importRowLabel, previewImport } from "@/features/imports/imports"
import type { CSVImportPreview, ImportSource } from "@/features/imports/imports"
import { invalidateGameRelated } from "@/features/games/games"
import { MythicTrackInstructions } from "@/features/imports/mythic-track-instructions"

export function ImportPage() {
  const queryClient = useQueryClient()
  const [source, setSource] = useState<ImportSource>("csv")
  const [payload, setPayload] = useState("")
  const preview = useMutation({
    mutationFn: (input: string) => previewImport(source, input),
  })
  const commit = useMutation({
    mutationFn: (input: string) => commitImport(source, input),
    onSuccess: () => {
      void invalidateGameRelated(queryClient)
    },
  })

  function updatePayload(value: string) {
    setPayload(value)
    preview.reset()
    commit.reset()
  }

  function switchSource(value: string) {
    setSource(value as ImportSource)
    updatePayload("")
  }

  async function chooseFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0]
    if (file) updatePayload(await file.text())
  }

  const data = preview.data
  const isCSV = source === "csv"

  return (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Import game history"
        description="Bring in past games from a CSV file or a Mythic Track export. Nothing is saved until you confirm the preview."
        actions={
          isCSV ? (
            <a className="btn btn-outline btn-sm" href="/api/imports/csv/sample" download>
              <Download className="size-4" /> Sample CSV
            </a>
          ) : null
        }
      />

      <Tabs value={source} onValueChange={switchSource}>
        <TabsList aria-label="Import source">
          <TabsTrigger value="csv">
            <FileSpreadsheet className="size-4" /> CSV
          </TabsTrigger>
          <TabsTrigger value="mythic_track">
            <FileJson className="size-4" /> Mythic Track
          </TabsTrigger>
        </TabsList>

        <TabsContent value="mythic_track">
          <MythicTrackInstructions />
        </TabsContent>

        <section className="card border-base-300 bg-base-200 border">
          <div className="card-body gap-4 p-4 sm:p-6">
            <label className="border-base-300 bg-base-100 hover:border-primary flex cursor-pointer flex-col items-center gap-2 rounded-xl border border-dashed p-6 text-center transition-colors">
              {isCSV ? (
                <FileSpreadsheet className="text-primary size-8" />
              ) : (
                <FileJson className="text-primary size-8" />
              )}
              <span className="font-semibold">
                {isCSV ? "Choose a CSV file" : "Choose mythic-track-games.json"}
              </span>
              <span className="text-base-content/60 text-sm">or paste its contents below</span>
              <input
                key={source}
                type="file"
                accept={isCSV ? ".csv,text/csv" : ".json,application/json"}
                className="sr-only"
                onChange={chooseFile}
              />
            </label>
            <label className="fieldset">
              <span className="fieldset-legend">{isCSV ? "CSV contents" : "JSON contents"}</span>
              <textarea
                className="textarea textarea-bordered min-h-40 w-full font-mono text-xs"
                value={payload}
                onChange={(event) => updatePayload(event.target.value)}
                placeholder={
                  isCSV
                    ? "game_id,date,player,deck,commander,seat,result…"
                    : '[{"id": "…", "gameStatus": 3, "players": [ … ] }]'
                }
              />
            </label>
            {(preview.error || (commit.error && !isSudoRequired(commit.error))) && (
              <div className="alert alert-error text-sm">
                {errorMessage(preview.error ?? commit.error) ?? "Import failed."}
              </div>
            )}
            <div className="card-actions justify-end">
              <button
                type="button"
                className="btn btn-primary"
                disabled={!payload.trim() || preview.isPending}
                onClick={() => preview.mutate(payload)}
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
      </Tabs>

      {data && <Preview preview={data} source={source} />}

      <SudoPrompt
        error={commit.error}
        onSuccess={() => commit.variables && commit.mutate(commit.variables)}
      />

      {data?.valid && !commit.data && (
        <div className="flex flex-col items-stretch justify-between gap-3 sm:flex-row sm:items-center">
          <p className="text-base-content/70 text-sm">
            Imports {count(data.games.length, "game")} in one transaction, creating any missing
            players and commander decks.
          </p>
          <button
            type="button"
            className="btn btn-success"
            disabled={commit.isPending}
            onClick={() => commit.mutate(payload)}
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

function Preview({ preview, source }: { preview: CSVImportPreview; source: ImportSource }) {
  const errorLines = new Set(preview.errors.map((error) => error.line))
  const rowLabel = importRowLabel[source]

  return (
    <section className="flex min-w-0 flex-col gap-4" aria-labelledby="preview-heading">
      <div className={`alert ${preview.valid ? "alert-success" : "alert-error"}`}>
        {preview.valid ? <CheckCircle2 className="size-5" /> : <XCircle className="size-5" />}
        <div>
          <h2 id="preview-heading" className="font-bold">
            {preview.valid
              ? `${preview.games.length} ${preview.games.length === 1 ? "game" : "games"} ready`
              : "Fix errors before importing"}
          </h2>
          <p className="text-sm">
            Creates {count(preview.players.create.length, "new player")} and{" "}
            {count(preview.decks.create.length, "commander deck")};{" "}
            {count(preview.players.matched.length, "player")} and{" "}
            {count(preview.decks.matched.length, "deck")} already exist and will be reused.
          </p>
        </div>
      </div>

      {preview.players.create.length > 0 && (
        <div className="card border-base-300 bg-base-200 border">
          <div className="card-body gap-2 p-4">
            <h3 className="text-sm font-bold">New players</h3>
            <p className="text-base-content/60 text-xs">
              Nobody needs an account to appear here. Later, link a player to an account from Admin
              → Users, or merge duplicates from a player's page.
            </p>
            <ul className="flex flex-wrap gap-2" aria-label="Players that will be created">
              {preview.players.create.map((player) => (
                <li key={playerName(player)} className="badge badge-outline">
                  {playerName(player)}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}

      {preview.warnings.length > 0 && (
        <div className="alert alert-warning items-start text-sm">
          <AlertTriangle className="size-5" />
          <div>
            <p className="font-bold">
              {preview.warnings.length} {preview.warnings.length === 1 ? "game" : "games"} will not
              be imported
            </p>
            <ul className="mt-1 list-inside list-disc" aria-label="Import warnings">
              {preview.warnings.map((warning, index) => (
                <li key={`${warning.line}-${index}`}>
                  {rowLabel} {warning.line}: {warning.message}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}

      {preview.errors.length > 0 && (
        <ul className="grid gap-2" aria-label="Import errors">
          {preview.errors.map((error, index) => (
            <li
              key={`${error.line}-${error.field}-${index}`}
              className="border-error/40 bg-error/10 rounded-lg border px-4 py-3 text-sm"
            >
              <strong>
                {rowLabel} {error.line}
              </strong>{" "}
              · {error.field}: {error.message}
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
                    <td className="max-w-40 truncate font-mono text-xs" title={game.game_id}>
                      {game.game_id}
                    </td>
                    <td>{new Date(game.played_at).toLocaleDateString()}</td>
                    <td>{seat.seat}</td>
                    <td className="font-semibold">{seat.player}</td>
                    <td>
                      {seat.deck}
                      <span className="text-base-content/60 block text-xs">
                        {seat.partner ? `${seat.commander} / ${seat.partner}` : seat.commander}
                      </span>
                    </td>
                    <td>
                      <span
                        className={`badge ${seat.result === "win" ? "badge-success" : seat.result === "draw" ? "badge-info" : "badge-ghost"}`}
                      >
                        {seat.result}
                      </span>
                      {seat.mvp_card && (
                        <span className="text-base-content/60 block text-xs">
                          MVP: {seat.mvp_card}
                        </span>
                      )}
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

function count(n: number, noun: string) {
  return `${n} ${noun}${n === 1 ? "" : "s"}`
}

function playerName(player: string | { name: string }) {
  return typeof player === "string" ? player : player.name
}
