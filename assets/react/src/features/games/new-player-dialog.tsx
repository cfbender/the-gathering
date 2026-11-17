import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { UserPlus } from "lucide-react"
import { useState, type FormEvent } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { invalidateGameRelated, type PlayerSummary } from "@/features/games/games"
import { api, ApiError } from "@/lib/api"

/** Opens a dialog that adds a player to the playgroup and opens their page. */
export function NewPlayerButton() {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button type="button" className="btn btn-primary btn-sm" onClick={() => setOpen(true)}>
        <UserPlus className="size-4" /> Add player
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Add player</DialogTitle>
            <DialogClose onClose={() => setOpen(false)} />
          </DialogHeader>
          {open && <NewPlayerForm onCancel={() => setOpen(false)} />}
        </DialogContent>
      </Dialog>
    </>
  )
}

function NewPlayerForm({ onCancel }: { onCancel: () => void }) {
  const [name, setName] = useState("")
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: PlayerSummary }>("/api/players", {
        method: "POST",
        body: JSON.stringify({ name: name.trim() }),
      }).then((body) => body.data),
    onSuccess: async (player) => {
      await invalidateGameRelated(queryClient)
      await navigate({ to: "/players/$playerId", params: { playerId: String(player.id) } })
    },
  })
  const error = mutation.error instanceof ApiError ? mutation.error : null
  const nameErrors = error?.fieldErrors("name") ?? []

  function submit(event: FormEvent) {
    event.preventDefault()
    mutation.mutate()
  }

  return (
    <form className="space-y-4 p-5" onSubmit={submit}>
      <label className="form-control flex flex-col gap-1">
        <span className="label-text text-sm font-medium">Name</span>
        <input
          className="input input-bordered w-full"
          value={name}
          required
          maxLength={100}
          autoFocus
          disabled={mutation.isPending}
          onChange={(event) => {
            setName(event.target.value)
            mutation.reset()
          }}
        />
      </label>
      {mutation.isError && (
        <div role="alert" className="alert alert-error text-sm">
          {nameErrors.length > 0
            ? `Name ${nameErrors.join(", ")}`
            : (error?.detail ?? "Could not add the player. Try again.")}
        </div>
      )}
      <div className="flex justify-end gap-2">
        <button
          type="button"
          className="btn btn-ghost"
          onClick={onCancel}
          disabled={mutation.isPending}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="btn btn-primary"
          disabled={!name.trim() || mutation.isPending}
        >
          {mutation.isPending ? "Adding…" : "Add player"}
        </button>
      </div>
    </form>
  )
}
