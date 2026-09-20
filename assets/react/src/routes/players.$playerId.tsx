import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { PlayerAvatar } from "@/components/player-avatar"
import { Merge, Trophy } from "lucide-react"
import { useState } from "react"
import { PlayerStats } from "@/components/stats/player-stats"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { errorMessage, useCurrentUser } from "@/lib/auth"
import { formatDate, getPlayer, getPlayers, mergePlayers } from "@/lib/games"
import type { Player } from "@/lib/games"

export const Route = createFileRoute("/players/$playerId")({ component: PlayerDetailPage })

function PlayerDetailPage() {
  const { playerId } = Route.useParams()
  const query = useQuery({ queryKey: ["players", playerId], queryFn: () => getPlayer(playerId) })
  if (query.isPending) return <span className="loading loading-spinner" />
  if (query.isError) return <div className="alert alert-error">Player not found.</div>
  const player = query.data
  return (
    <div className="flex flex-col gap-8">
      <PageHeader
        eyebrow="Player profile"
        title={player.name}
        actions={<PlayerAvatar name={player.name} avatarUrl={player.avatar_url} size="lg" />}
      >
        <div className="stats border-base-300 bg-base-100/60 mt-5 border">
          <div className="stat">
            <div className="stat-title">Games</div>
            <div className="stat-value text-2xl">{player.games_played}</div>
          </div>
          <div className="stat">
            <div className="stat-title">Wins</div>
            <div className="stat-value text-success text-2xl">{player.wins}</div>
          </div>
        </div>
      </PageHeader>
      <PlayerStats playerId={playerId} />
      <section>
        <h2 className="mb-3 text-xl font-bold">Decks</h2>
        {player.decks?.length === 0 && (
          <p className="text-base-content/60">No decks recorded yet.</p>
        )}
        <div className="grid gap-3 sm:grid-cols-2">
          {player.decks?.map((deck) => (
            <Link
              key={deck.id}
              to="/decks/$deckId"
              params={{ deckId: String(deck.id) }}
              className="card border-base-300 bg-base-200 border"
            >
              <div className="card-body p-4">
                <strong>{deck.name}</strong>
                <span className="text-base-content/60 text-sm">{deck.commander_name}</span>
              </div>
            </Link>
          ))}
        </div>
      </section>
      <section>
        <h2 className="mb-3 text-xl font-bold">Recent games</h2>
        {player.recent_games?.length === 0 && (
          <p className="text-base-content/60">No games played yet.</p>
        )}
        <div className="divide-base-300 border-base-300 bg-base-200 divide-y rounded-lg border">
          {player.recent_games?.map((game) => (
            <Link
              key={game.id}
              to="/games/$gameId"
              params={{ gameId: String(game.id) }}
              className="flex items-center justify-between p-4"
            >
              <span>
                <strong>{formatDate(game.played_at)}</strong>
                <span className="text-base-content/60 block text-sm">
                  {game.deck?.name ?? "Unknown deck"}
                </span>
              </span>
              {game.result === "win" && (
                <span className="text-success flex items-center gap-1 font-bold">
                  <Trophy className="size-4" /> Win
                </span>
              )}
            </Link>
          ))}
        </div>
      </section>
      <MergePlayer player={player} />
    </div>
  )
}

/**
 * Administrators fold duplicate players (an imported guest and the same person's
 * account, say) into one. Everything moves to the chosen player; this one goes away.
 */
function MergePlayer({ player }: { player: Player }) {
  const viewer = useCurrentUser()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const players = useQuery({
    queryKey: ["players"],
    queryFn: getPlayers,
    enabled: viewer.data?.role === "admin",
  })
  const [targetId, setTargetId] = useState("")
  const [confirming, setConfirming] = useState(false)
  const merge = useMutation({
    mutationFn: (target: Player) => mergePlayers(player.id, target.id),
    onSuccess: (target) => {
      void queryClient.invalidateQueries({ queryKey: ["players"] })
      void queryClient.invalidateQueries({ queryKey: ["games"] })
      void queryClient.invalidateQueries({ queryKey: ["decks"] })
      void queryClient.invalidateQueries({ queryKey: ["stats"] })
      void navigate({ to: "/players/$playerId", params: { playerId: String(target.id) } })
    },
  })

  if (viewer.data?.role !== "admin") return null
  const options = (players.data ?? []).filter((item) => item.id !== player.id)
  const target = options.find((item) => String(item.id) === targetId)

  return (
    <section className="card border-base-300 bg-base-200 border" aria-labelledby="merge-heading">
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="merge-heading" className="flex items-center gap-2 text-lg font-bold">
          <Merge className="size-5" /> Merge into another player
        </h2>
        <p className="text-base-content/70 text-sm">
          Use this when {player.name} and another player are the same person, for example a guest
          from an import and their account. {player.name}'s games, decks, and account link move to
          the player you choose, and {player.name} is removed.
        </p>
        <div className="flex flex-col gap-2 sm:flex-row">
          <select
            aria-label="Player to merge into"
            className="select min-w-0 flex-1"
            value={targetId}
            onChange={(event) => setTargetId(event.target.value)}
          >
            <option value="">Choose a player…</option>
            {options.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="btn btn-outline btn-error"
            disabled={!target || merge.isPending}
            onClick={() => setConfirming(true)}
          >
            Merge
          </button>
        </div>
        {merge.error && (
          <p className="text-error text-sm">
            {errorMessage(merge.error, "merge") ?? errorMessage(merge.error)}
          </p>
        )}
      </div>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Merge ${player.name} into ${target?.name ?? ""}?`}
        confirmLabel="Merge players"
        destructive
        onConfirm={() => target && merge.mutate(target)}
      >
        {player.games_played ?? 0} games and {player.decks?.length ?? 0} decks move to{" "}
        <strong>{target?.name}</strong>. <strong>{player.name}</strong> is deleted. This cannot be
        undone.
      </ConfirmDialog>
    </section>
  )
}
