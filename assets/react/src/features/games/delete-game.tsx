import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { Trash2 } from "lucide-react"
import { useState } from "react"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { ApiError } from "@/lib/api"
import { deleteGame, invalidateGameRelated, type Game } from "./games"

export function DeleteGame({ game }: { game: Game }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [confirming, setConfirming] = useState(false)
  const remove = useMutation({
    mutationFn: () => deleteGame(game.id),
    onSuccess: async () => {
      queryClient.removeQueries({ queryKey: ["games", String(game.id)] })
      await invalidateGameRelated(queryClient)
      await navigate({ to: "/games", search: {} })
    },
  })

  return (
    <>
      <button
        type="button"
        className="btn btn-outline btn-error"
        disabled={remove.isPending}
        onClick={() => setConfirming(true)}
      >
        <Trash2 className="size-4" /> {remove.isPending ? "Deleting…" : "Delete game"}
      </button>
      {remove.isPending && (
        <span role="status" className="sr-only">
          Deleting game
        </span>
      )}
      {remove.isError && (
        <p role="alert" className="text-error basis-full text-sm">
          {(remove.error instanceof ApiError && remove.error.detail) ||
            "Could not delete the game."}
        </p>
      )}
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Delete this game?"
        confirmLabel="Delete game"
        destructive
        onConfirm={() => remove.mutate()}
      >
        This game and all of its recorded seats will be deleted. This cannot be undone.
      </ConfirmDialog>
    </>
  )
}
