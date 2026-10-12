import { useMutation, useQueryClient } from "@tanstack/react-query"
import { FileJson, Upload } from "lucide-react"
import { useState } from "react"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { invalidateGameRelated } from "@/features/games/games"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"

type TransferCounts = Record<"players" | "decks" | "games", { created: number; reused: number }>

async function transfer(json: string, preview: boolean) {
  const response = await api<{ data: TransferCounts }>(
    `/api/imports/portable${preview ? "/preview" : ""}`,
    { method: "POST", body: JSON.stringify({ json }) },
  )
  return response.data
}

export function PortableTransfer() {
  const queryClient = useQueryClient()
  const [payload, setPayload] = useState("")
  const [fileError, setFileError] = useState<string | null>(null)
  const preview = useMutation({ mutationFn: (json: string) => transfer(json, true) })
  const commit = useMutation({
    mutationFn: (json: string) => transfer(json, false),
    onSuccess: () => {
      void invalidateGameRelated(queryClient)
    },
  })
  const busy = preview.isPending || commit.isPending
  const error = preview.error ?? (isSudoRequired(commit.error) ? null : commit.error)
  const counts = commit.data ?? preview.data
  const creates = preview.data && Object.values(preview.data).some((count) => count.created > 0)

  return (
    <div className="mx-auto flex w-full max-w-5xl flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Transfer history from another instance"
        description="Import a portable JSON file. Any member can download one from the Games page’s … menu."
      />
      <section className="card border-base-300 bg-base-200 border">
        <div className="card-body gap-4">
          <FileJson className="text-primary size-7" aria-hidden="true" />
          <h2 className="card-title">Import a Gathering export</h2>
          <p className="text-base-content/75 text-sm">
            Preview what will be added before saving. Existing games are skipped. Same-name players
            and decks are reused without replacing their settings.
          </p>
          <label className="fieldset">
            <span className="fieldset-legend">The Gathering JSON file</span>
            <input
              type="file"
              accept=".json,application/json"
              disabled={busy}
              className="file-input w-full"
              onChange={async (event) => {
                preview.reset()
                commit.reset()
                setPayload("")
                setFileError(null)
                const input = event.currentTarget
                const file = input.files?.[0]
                if (!file) return
                try {
                  const text = await file.text()
                  if (input.files?.[0] === file) setPayload(text)
                } catch {
                  if (input.files?.[0] === file)
                    setFileError("Could not read that file. Choose it again.")
                }
              }}
            />
          </label>
          <button
            className="btn btn-outline"
            disabled={!payload || busy}
            onClick={() => preview.mutate(payload)}
          >
            <Upload className="size-4" />
            {preview.isPending ? "Checking export…" : "Preview transfer"}
          </button>
          {(fileError || error) && (
            <p role="alert" className="text-error break-words text-sm">
              {fileError ??
                errorMessage(error, "import") ??
                errorMessage(error) ??
                "Import failed."}
            </p>
          )}
        </div>
      </section>
      {counts && (
        <section className="card border-base-300 bg-base-200 border">
          <div className="card-body gap-4">
            <h2 className="card-title">{commit.data ? "Transfer complete" : "Transfer preview"}</h2>
            <table className="table w-full">
              <caption className="sr-only">Records added and existing records kept</caption>
              <thead>
                <tr>
                  <th>Records</th>
                  <th>{commit.data ? "Added" : "To add"}</th>
                  <th>Already here</th>
                </tr>
              </thead>
              <tbody>
                {(["players", "decks", "games"] as const).map((kind) => (
                  <tr key={kind}>
                    <th className="capitalize">{kind}</th>
                    <td>{counts[kind].created}</td>
                    <td>{counts[kind].reused}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="text-base-content/70 text-sm">
              No existing records will be overwritten or deleted. Players match by name; decks by
              owner and name. Conflicting commanders block the transfer. New players can be linked
              to accounts afterward.
            </p>
            {commit.data ? (
              <p role="status" className="text-success">
                Imported successfully. Re-importing this file will not duplicate games.
              </p>
            ) : (
              <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-base-content/70 text-sm">
                  {creates
                    ? "Everything is saved together, or nothing is saved."
                    : "No new players, decks or games. Existing history will be kept."}
                </p>
                <button
                  className="btn btn-success"
                  disabled={busy || preview.variables !== payload}
                  onClick={() => commit.mutate(payload)}
                >
                  {commit.isPending ? "Importing…" : "Confirm transfer"}
                </button>
              </div>
            )}
          </div>
        </section>
      )}
      <SudoPrompt
        error={commit.error}
        onSuccess={() => commit.variables && commit.mutate(commit.variables)}
      />
    </div>
  )
}
