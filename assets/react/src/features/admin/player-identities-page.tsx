import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { Unlink } from "lucide-react"
import { useState } from "react"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { invalidateGameRelated } from "@/features/games/games"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"

interface PlayerIdentity {
  id: number
  name: string
  discord_id: string | null
  archived_at: string | null
  user: { id: number; username: string } | null
}

interface IdentityPage {
  data: PlayerIdentity[]
  meta: { page: number; total: number; total_pages: number }
}

export function PlayerIdentitiesPage() {
  const queryClient = useQueryClient()
  const [search, setSearch] = useState("")
  const [page, setPage] = useState(1)
  const [pending, setPending] = useState<PlayerIdentity | null>(null)
  const players = useQuery({
    queryKey: ["players", "identities", { search, page }],
    queryFn: () =>
      api<IdentityPage>(
        `/api/admin/players?${new URLSearchParams({ search, page: String(page) })}`,
      ),
  })
  const unlink = useMutation({
    mutationFn: (player: PlayerIdentity) =>
      api<void>(`/api/admin/players/${player.id}/identity`, { method: "DELETE" }),
    onSuccess: () => void invalidateGameRelated(queryClient),
  })

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Player identities"
        description="Review Discord identities and account links, including archived players. Unlink an incorrect identity before merging players."
      />
      <label className="form-control flex flex-col gap-2">
        <span className="text-sm font-medium">Search players, Discord IDs, or usernames</span>
        <input
          type="search"
          className="input w-full sm:max-w-lg"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value)
            setPage(1)
          }}
        />
      </label>
      {players.isPending && <p role="status">Loading player identities…</p>}
      {players.error && !isSudoRequired(players.error) && (
        <div role="alert" className="alert alert-error">
          {errorMessage(players.error)}
        </div>
      )}
      <SudoPrompt error={players.error} onSuccess={() => void players.refetch()} />
      {unlink.error && !isSudoRequired(unlink.error) && (
        <div role="alert" className="alert alert-error">
          {errorMessage(unlink.error)}
        </div>
      )}
      <SudoPrompt
        error={unlink.error}
        onSuccess={() => unlink.variables && unlink.mutate(unlink.variables)}
      />
      {unlink.isSuccess && (
        <p role="status" className="text-success text-sm">
          Identity unlinked from {unlink.variables.name}. Their games and decks are unchanged.
        </p>
      )}
      {players.data && !players.error && (
        <>
          <ul className="grid gap-3">
            {players.data.data.map((player) => (
              <li key={player.id} className="card border-base-300 bg-base-200 border">
                <div className="card-body gap-4 p-4 sm:flex-row sm:items-center sm:justify-between sm:p-5">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <Link
                        to="/players/$playerId"
                        params={{ playerId: String(player.id) }}
                        className="link link-hover break-words font-semibold"
                      >
                        {player.name}
                      </Link>
                      {player.archived_at && (
                        <span className="badge badge-outline badge-sm">Archived</span>
                      )}
                    </div>
                    <dl className="mt-2 grid gap-1 text-sm">
                      <div className="flex flex-wrap gap-x-2">
                        <dt className="text-base-content/60">Discord ID</dt>
                        <dd className="break-all font-mono">{player.discord_id ?? "Not linked"}</dd>
                      </div>
                      <div className="flex flex-wrap gap-x-2">
                        <dt className="text-base-content/60">Account</dt>
                        <dd className="break-all">
                          {player.user ? `@${player.user.username}` : "No account"}
                        </dd>
                      </div>
                    </dl>
                  </div>
                  <button
                    type="button"
                    className="btn btn-outline btn-sm self-start sm:self-center"
                    aria-label={`Unlink identity for ${player.name}`}
                    disabled={(!player.discord_id && !player.user) || unlink.isPending}
                    onClick={() => setPending(player)}
                  >
                    <Unlink className="size-4" /> Unlink identity
                  </button>
                </div>
              </li>
            ))}
          </ul>
          {players.data.data.length === 0 && <p>No players found.</p>}
          <nav
            aria-label="Player identity pages"
            className="flex flex-wrap items-center justify-between gap-3"
          >
            <p className="text-base-content/60 text-sm">
              {players.data.meta.total} players · Page {page} of {players.data.meta.total_pages}
            </p>
            <div className="flex gap-2">
              <button className="btn btn-sm" disabled={page <= 1} onClick={() => setPage(page - 1)}>
                Previous
              </button>
              <button
                className="btn btn-sm"
                disabled={page >= players.data.meta.total_pages}
                onClick={() => setPage(page + 1)}
              >
                Next
              </button>
            </div>
          </nav>
        </>
      )}
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(open) => !open && setPending(null)}
        title={`Unlink identity from ${pending?.name ?? ""}?`}
        confirmLabel="Unlink identity"
        destructive
        onConfirm={() => pending && unlink.mutate(pending)}
      >
        The Discord identity{pending?.discord_id ? ` ${pending.discord_id}` : ""} and any linked
        account will be detached from this player. Games and decks stay with the player. The account
        can still sign in; a future Discord sign-in or import may create a separate player. You can
        link the account to the correct player in Users.
      </ConfirmDialog>
    </div>
  )
}
