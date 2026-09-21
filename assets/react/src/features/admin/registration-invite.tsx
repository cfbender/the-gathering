import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link2 } from "lucide-react"
import { useState } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"

export function RegistrationInvite() {
  const queryClient = useQueryClient()
  const [confirmRotate, setConfirmRotate] = useState(false)
  const [copyStatus, setCopyStatus] = useState("")
  const status = useQuery({
    queryKey: ["admin", "registration-invite"],
    queryFn: async () =>
      (await api<{ data: { enabled: boolean } }>("/api/admin/registration-invite")).data,
  })
  const rotate = useMutation({
    gcTime: 0,
    mutationFn: async () =>
      (await api<{ data: { token: string } }>("/api/admin/registration-invite", { method: "POST" }))
        .data,
    onSuccess: () => {
      setCopyStatus("")
      void queryClient.invalidateQueries({ queryKey: ["admin", "registration-invite"] })
    },
  })
  const link = rotate.data
    ? `${window.location.origin}/invite#token=${rotate.data.token}`
    : undefined
  const error = rotate.error ?? status.error

  async function copyLink() {
    if (!link) return
    try {
      await navigator.clipboard.writeText(link)
      setCopyStatus("Link copied.")
    } catch {
      setCopyStatus("Could not copy automatically. Select and copy the link above.")
    }
  }

  return (
    <section className="card bg-base-200 border-base-300 border" aria-labelledby="invite-heading">
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="invite-heading" className="flex items-center gap-2 text-lg font-semibold">
          <Link2 className="size-5" /> Sign-up invitation
        </h2>
        <p className="text-base-content/70 text-sm">
          Anyone with this link can join with Discord, even when open registration is off. It can be
          reused and does not expire. Share it only with people you want in your playgroup.
        </p>
        <p className="text-base-content/70 text-sm">
          {status.data?.enabled
            ? "A link is active. Rotating it stops the old link from allowing new registrations, including sign-ups still at Discord. Existing members keep their access."
            : "Create a link to invite new members without opening registration to everyone."}
        </p>
        {link && (
          <div className="grid gap-2">
            <label className="text-sm font-medium" htmlFor="registration-invite-link">
              Shareable sign-up link
            </label>
            <div className="flex flex-col gap-2 sm:flex-row">
              <input
                id="registration-invite-link"
                className="input min-w-0 flex-1"
                value={link}
                readOnly
                onFocus={(event) => event.target.select()}
              />
              <button type="button" className="btn btn-primary" onClick={() => void copyLink()}>
                Copy link
              </button>
            </div>
            <p className="text-base-content/65 text-sm">
              Save this link now. For security, it cannot be shown again after you leave this page.
            </p>
            {copyStatus && (
              <p role="status" className="text-sm">
                {copyStatus}
              </p>
            )}
          </div>
        )}
        <div>
          <button
            type="button"
            className="btn btn-outline"
            disabled={!status.data || rotate.isPending}
            onClick={() => (status.data?.enabled ? setConfirmRotate(true) : rotate.mutate())}
          >
            {rotate.isPending ? "Creating…" : status.data?.enabled ? "Rotate link" : "Create link"}
          </button>
        </div>
        {error && !isSudoRequired(error) && (
          <p className="text-error text-sm">{errorMessage(error)}</p>
        )}
        <SudoPrompt
          error={error}
          onSuccess={() => (rotate.error ? rotate.mutate() : void status.refetch())}
        />
      </div>
      <ConfirmDialog
        open={confirmRotate}
        onOpenChange={setConfirmRotate}
        title="Rotate sign-up link?"
        confirmLabel="Rotate link"
        destructive
        onConfirm={() => rotate.mutate()}
      >
        The old link will stop allowing new registrations immediately. Send the new link to anyone
        who has not finished signing up.
      </ConfirmDialog>
    </section>
  )
}
