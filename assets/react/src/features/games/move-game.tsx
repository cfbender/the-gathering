import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { ArrowRightLeft } from "lucide-react"
import { useState } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { playgroupsQueryOptions, type Playgroup } from "@/features/playgroups/playgroups"
import { errorMessage, isSudoRequired, useCurrentUser } from "@/lib/auth"
import { invalidateGameRelated, moveGame, type Game } from "./games"

/** Admin only: moves the game into another playgroup the admin belongs to. */
export function MoveGame({ game }: { game: Game }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const viewer = useCurrentUser()
  const playgroups = useQuery({
    ...playgroupsQueryOptions,
    enabled: viewer.data?.role === "admin",
  })
  const [targetId, setTargetId] = useState("")
  const [confirming, setConfirming] = useState(false)
  const move = useMutation({
    mutationFn: (target: Playgroup) => moveGame(game.id, target.id),
    onSuccess: async () => {
      queryClient.removeQueries({ queryKey: ["games", String(game.id)] })
      await invalidateGameRelated(queryClient)
      await navigate({ to: "/games", search: {} })
    },
  })

  if (viewer.data?.role !== "admin") return null
  const options = (playgroups.data ?? []).filter((item) => item.id !== viewer.data?.playgroup_id)
  if (options.length === 0) return null
  const target = options.find((item) => String(item.id) === targetId)
  const seatError = errorMessage(move.error, "seats")

  return (
    <section className="card border-base-300 bg-base-200 border" aria-labelledby="move-heading">
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="move-heading" className="flex items-center gap-2 text-lg font-bold">
          <ArrowRightLeft className="size-5" /> Move to another playgroup
        </h2>
        <p className="text-base-content/70 text-sm">
          Use this when the game was recorded in the wrong playgroup. Each seat is matched to that
          playgroup's player with the same account, Discord identity, or name, and to their deck of
          the same name; players and decks that do not exist there yet are created.
        </p>
        <div className="flex flex-col gap-2 sm:flex-row">
          <select
            aria-label="Playgroup to move into"
            className="select min-w-0 flex-1"
            value={targetId}
            onChange={(event) => setTargetId(event.target.value)}
          >
            <option value="">Choose a playgroup…</option>
            {options.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="btn btn-outline"
            disabled={!target || move.isPending}
            onClick={() => setConfirming(true)}
          >
            {move.isPending ? "Moving…" : "Move"}
          </button>
        </div>
        {move.error && !isSudoRequired(move.error) && (
          <p role="alert" className="text-error text-sm">
            {seatError ??
              errorMessage(move.error, "playgroup_id") ??
              errorMessage(move.error) ??
              "Could not move the game."}
          </p>
        )}
        <SudoPrompt
          error={move.error}
          onSuccess={() => move.variables && move.mutate(move.variables)}
        />
      </div>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Move this game to ${target?.name ?? ""}?`}
        confirmLabel="Move game"
        onConfirm={() => target && move.mutate(target)}
      >
        The game leaves this playgroup and appears in <strong>{target?.name}</strong> with its seats
        re-pointed to players and decks there. The players and decks in this playgroup are left as
        they are.
      </ConfirmDialog>
    </section>
  )
}
