import { useMutation } from "@tanstack/react-query"
import type { FormEvent } from "react"
import { LockKeyhole } from "lucide-react"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import { formValue } from "@/lib/form"

export function SudoPrompt({ error, onSuccess }: { error: unknown; onSuccess: () => void }) {
  const sudo = useMutation({
    mutationFn: (password: string) =>
      api("/api/session/sudo", { method: "POST", body: JSON.stringify({ password }) }),
    onSuccess,
  })

  if (!isSudoRequired(error)) return null

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    sudo.mutate(formValue(event.currentTarget, "password"))
  }

  return (
    <div
      role="alertdialog"
      aria-labelledby="sudo-heading"
      className="card border-warning bg-base-200 border shadow-sm"
    >
      <form className="card-body gap-4 sm:flex-row sm:items-end" onSubmit={submit}>
        <div className="flex flex-1 gap-3">
          <LockKeyhole className="text-warning mt-1 size-5 shrink-0" aria-hidden="true" />
          <div>
            <h2 id="sudo-heading" className="font-semibold">
              Confirm it’s you
            </h2>
            <p className="text-base-content/70 text-sm">
              Enter your password to continue this sensitive action.
            </p>
          </div>
        </div>
        <div className="flex flex-col gap-2 sm:min-w-64">
          <input
            name="password"
            type="password"
            autoComplete="current-password"
            className="input input-sm w-full"
            aria-label="Confirm your password"
            required
            autoFocus
          />
          {sudo.error && <span className="text-error text-sm">{errorMessage(sudo.error)}</span>}
          <button className="btn btn-warning btn-sm" disabled={sudo.isPending}>
            {sudo.isPending ? "Confirming…" : "Confirm password"}
          </button>
        </div>
      </form>
    </div>
  )
}
