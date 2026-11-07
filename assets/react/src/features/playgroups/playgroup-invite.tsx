import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link2 } from "lucide-react"
import { useState } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import { playgroupInviteQueryOptions, rotatePlaygroupInvite } from "./playgroups"

/** Administers one playgroup's reusable sign-up link. The secret is shown once, after
 * creating or rotating it; the server keeps only its digest. */
export function PlaygroupInvite({ playgroupId, name }: { playgroupId: number; name: string }) {
  const queryClient = useQueryClient()
  const [confirmRotate, setConfirmRotate] = useState(false)
  const [copyStatus, setCopyStatus] = useState("")
  const status = useQuery(playgroupInviteQueryOptions(playgroupId))
  const rotate = useMutation({
    gcTime: 0,
    mutationFn: () => rotatePlaygroupInvite(playgroupId),
    onSuccess: () => {
      setCopyStatus("")
      void queryClient.invalidateQueries({ queryKey: ["admin", "playgroups"] })
    },
  })
  const link = rotate.data
    ? `${window.location.origin}/invite#token=${rotate.data.token}`
    : undefined
  const error = rotate.error ?? status.error
  const inputId = `playgroup-${playgroupId}-invite-link`

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
    <section
      className="border-base-300 bg-base-100/60 rounded-box flex flex-col gap-3 border p-4"
      aria-label={`Invite link for ${name}`}
    >
      <h3 className="flex items-center gap-2 font-semibold">
        <Link2 className="size-4" /> Invite link
      </h3>
      <p className="text-base-content/70 text-sm">
        {status.data?.enabled
          ? `A link is active. Anyone with it joins ${name} with Discord, even when open registration is off; members who already have an account are added to this playgroup. Rotating it stops the old link immediately. Existing members keep their access.`
          : `Create a link so people can join ${name} with Discord without opening registration to everyone. It can be reused and does not expire.`}
      </p>
      {link && (
        <div className="grid gap-2">
          <label className="text-sm font-medium" htmlFor={inputId}>
            Shareable invite link
          </label>
          <div className="flex flex-col gap-2 sm:flex-row">
            <input
              id={inputId}
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
          className="btn btn-outline btn-sm"
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
      <ConfirmDialog
        open={confirmRotate}
        onOpenChange={setConfirmRotate}
        title={`Rotate the invite link for ${name}?`}
        confirmLabel="Rotate link"
        destructive
        onConfirm={() => rotate.mutate()}
      >
        The old link will stop working immediately. Send the new link to anyone who has not finished
        joining.
      </ConfirmDialog>
    </section>
  )
}
