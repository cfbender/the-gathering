import { useMutation, useQueryClient } from "@tanstack/react-query"
import { RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { invalidateGameRelated } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"
import {
  hasDeckHost,
  remoteDeckSourceLabels,
  syncRemoteDecks,
  type RemoteDeckSyncResult,
} from "@/lib/remote-decks"

/**
 * Pulls the signed-in user's hosted decks into their player's deck list. Renders
 * nothing when they have no deck host configured; the result alert is exposed
 * separately so pages can place it where it reads best.
 */
export function useSyncRemoteDecks(onSuccess?: () => void) {
  const queryClient = useQueryClient()
  const user = useCurrentUser()
  const mutation = useMutation({
    mutationFn: syncRemoteDecks,
    onSuccess: async () => {
      onSuccess?.()
      await invalidateGameRelated(queryClient)
      void queryClient.invalidateQueries({ queryKey: ["deck-chooser"] })
    },
  })
  return { ...mutation, available: hasDeckHost(user.data) }
}

export function SyncRemoteDecksButton({
  sync,
  size,
}: {
  sync: ReturnType<typeof useSyncRemoteDecks>
  size?: "sm"
}) {
  if (!sync.available) return null
  return (
    <Button variant="outline" size={size} onClick={() => sync.mutate()} disabled={sync.isPending}>
      <RefreshCw className={sync.isPending ? "size-4 animate-spin" : "size-4"} />
      {sync.isPending ? "Syncing…" : "Sync hosted decks"}
    </Button>
  )
}

export function SyncRemoteDecksResult({ sync }: { sync: ReturnType<typeof useSyncRemoteDecks> }) {
  if (sync.isError) {
    return (
      <div role="alert" className="alert alert-error">
        Your hosted decks could not be synced. Check your deck hosts in Settings, then try again.
      </div>
    )
  }
  if (!sync.isSuccess) return null
  return <SyncSummary result={sync.data} />
}

function SyncSummary({ result }: { result: RemoteDeckSyncResult }) {
  const partial = result.errors.length > 0
  return (
    <div role="status" className={partial ? "alert alert-warning" : "alert alert-success"}>
      <RefreshCw className="size-5" />
      <span>
        Synced hosted decks: {result.created} added, {result.updated} linked or updated.
        {result.errors.map((failure) => (
          <span key={failure.source} className="block text-sm">
            {remoteDeckSourceLabels[failure.source]} was skipped: {failure.error}
          </span>
        ))}
      </span>
    </div>
  )
}
