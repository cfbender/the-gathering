import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Braces } from "lucide-react"
import { useState, type FormEvent } from "react"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { formatDate } from "@/features/games/games"
import { api } from "@/lib/api"
import { errorMessage } from "@/lib/auth"
import { formValue } from "@/lib/form"

export interface ApiKey {
  id: number
  name: string
  prefix: string
  last_used_at: string | null
  inserted_at: string
}

interface CreatedApiKey extends ApiKey {
  token: string
}

const apiKeysKey = ["api-keys"] as const

/** Personal API keys for reading game history with the owner's permissions from scripts. */
export function ApiKeysSection() {
  const queryClient = useQueryClient()
  const [revoking, setRevoking] = useState<ApiKey | null>(null)
  const [copyStatus, setCopyStatus] = useState("")
  const keys = useQuery({
    queryKey: apiKeysKey,
    queryFn: async () => (await api<{ data: ApiKey[] }>("/api/session/api-keys")).data,
  })
  const create = useMutation({
    // Keep the one-time secret out of the mutation cache once this section unmounts.
    gcTime: 0,
    mutationFn: async (name: string) =>
      (
        await api<{ data: CreatedApiKey }>("/api/session/api-keys", {
          method: "POST",
          body: JSON.stringify({ name }),
        })
      ).data,
    onSuccess: () => {
      setCopyStatus("")
      void queryClient.invalidateQueries({ queryKey: apiKeysKey })
    },
  })
  const revoke = useMutation({
    mutationFn: (key: ApiKey) => api<void>(`/api/session/api-keys/${key.id}`, { method: "DELETE" }),
    onSuccess: (_result, key) => {
      if (create.data?.id === key.id) create.reset()
      void queryClient.invalidateQueries({ queryKey: apiKeysKey })
    },
  })
  const created = create.data

  function createKey(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    create.mutate(formValue(form, "name"), { onSuccess: () => form.reset() })
  }

  async function copyToken() {
    if (!created) return
    try {
      await navigator.clipboard.writeText(created.token)
      setCopyStatus("Key copied.")
    } catch {
      setCopyStatus("Could not copy automatically. Select and copy the key above.")
    }
  }

  return (
    <section className="card bg-base-200 border-base-300 min-w-0 border" aria-labelledby="api-keys">
      <div className="card-body min-w-0 gap-4">
        <div>
          <h2 id="api-keys" className="card-title text-lg">
            <Braces className="size-5" /> API keys
          </h2>
          <p className="text-base-content/60 mt-1 text-sm">
            Read your playgroup&apos;s game history from scripts and other tools. A key acts as you,
            with the same permissions you have, and stops working if your account is disabled.
          </p>
        </div>

        <pre className="bg-base-100/60 rounded-box overflow-x-auto p-3 text-xs">
          <code>{`curl -H "Authorization: Bearer <key>" \\
  "${window.location.origin}/api/v1/games?player_id=me&date_from=2026-01-01"`}</code>
        </pre>
        <p className="text-base-content/60 -mt-2 text-sm">
          Filters: <code>player_id</code> (an ID or <code>me</code>), <code>date_from</code> and{" "}
          <code>date_to</code> (YYYY-MM-DD, inclusive), <code>tz</code>, <code>page</code>, and{" "}
          <code>per_page</code> (up to 100).
        </p>

        <form className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-end" onSubmit={createKey}>
          <label className="fieldset min-w-0 flex-1 py-0">
            <span className="fieldset-legend">Key name</span>
            <input
              name="name"
              className="input min-w-0 w-full"
              placeholder="Game log spreadsheet"
              maxLength={60}
              required
            />
          </label>
          <button className="btn btn-primary" disabled={create.isPending}>
            {create.isPending ? "Creating…" : "Create key"}
          </button>
        </form>
        {create.error && (
          <p className="text-error -mt-2 text-sm">
            {errorMessage(create.error, "name") ?? errorMessage(create.error)}
          </p>
        )}

        {created && (
          <div className="border-success/40 bg-success/10 rounded-box grid gap-2 border p-3">
            <label className="text-sm font-medium" htmlFor="new-api-key">
              New key for {created.name}
            </label>
            <div className="flex min-w-0 flex-col gap-2 sm:flex-row">
              <input
                id="new-api-key"
                className="input min-w-0 flex-1 font-mono text-sm"
                value={created.token}
                readOnly
                onFocus={(event) => event.target.select()}
              />
              <button type="button" className="btn btn-outline" onClick={() => void copyToken()}>
                Copy key
              </button>
            </div>
            <p className="text-base-content/70 text-sm">
              Save this key now. It cannot be shown again after you leave this page.
            </p>
            {copyStatus && (
              <p role="status" className="text-sm">
                {copyStatus}
              </p>
            )}
          </div>
        )}

        {keys.data && keys.data.length > 0 ? (
          <ul className="divide-base-300 border-base-300 rounded-box divide-y border">
            {keys.data.map((key) => (
              <li key={key.id} className="flex min-w-0 items-center gap-3 p-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{key.name}</p>
                  <p className="text-base-content/60 text-xs">
                    <code>{key.prefix}…</code> · Created {formatDate(key.inserted_at)} ·{" "}
                    {key.last_used_at ? `Last used ${formatDate(key.last_used_at)}` : "Never used"}
                  </p>
                </div>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm text-error"
                  aria-label={`Revoke ${key.name}`}
                  disabled={revoke.isPending}
                  onClick={() => setRevoking(key)}
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        ) : (
          keys.isSuccess && <p className="text-base-content/60 text-sm">You have no API keys.</p>
        )}
        {(keys.error ?? revoke.error) && (
          <p className="text-error text-sm">{errorMessage(keys.error ?? revoke.error)}</p>
        )}
      </div>
      <ConfirmDialog
        open={revoking !== null}
        onOpenChange={(open) => !open && setRevoking(null)}
        title={`Revoke ${revoking?.name ?? "key"}?`}
        confirmLabel="Revoke key"
        destructive
        onConfirm={() => revoking && revoke.mutate(revoking)}
      >
        Anything using this key loses access immediately. This cannot be undone.
      </ConfirmDialog>
    </section>
  )
}
