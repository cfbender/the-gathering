import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { PlayerAvatar } from "@/components/player-avatar"
import { SudoPrompt } from "@/components/sudo-prompt"
import { Archive, ExternalLink, Merge, Trophy } from "lucide-react"
import { useState } from "react"
import { PlayerStats } from "@/components/stats/player-stats"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { CardArtBackground } from "@/components/card-art-background"
import {
  SyncRemoteDecksButton,
  SyncRemoteDecksResult,
  useSyncRemoteDecks,
} from "@/features/decks/sync-remote-decks"
import { isRetired, type DeckSummary } from "@/features/decks/decks"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { remoteDeckSourceLabels, type RemoteDeckSource } from "@/lib/remote-decks"
import { errorMessage, isSudoRequired, useCurrentUser } from "@/lib/auth"
import { formatDate, getPlayer, getPlayers, mergePlayers } from "@/features/games/games"
import type { PlayerDetail, PlayerSummary } from "@/features/games/games"

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
      <PlayerDecks player={player} />
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
 * The player's decks, with hosted decks (Moxfield, Archidekt, ManaVault) folded in:
 * the owner can sync them, which links same-commander decks and adds the rest.
 */
function PlayerDecks({ player }: { player: PlayerDetail }) {
  const viewer = useCurrentUser()
  const owner = viewer.data !== undefined && viewer.data.id === player.user_id
  const sync = useSyncRemoteDecks()
  const active = player.decks.filter((deck) => !isRetired(deck))
  const retired = player.decks.filter(isRetired)

  return (
    <section>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-xl font-bold">Decks</h2>
        {owner &&
          (sync.available ? (
            <SyncRemoteDecksButton sync={sync} size="sm" />
          ) : (
            <Link to="/settings" className="link link-hover text-base-content/65 text-sm">
              Connect a deck host to sync your hosted decks
            </Link>
          ))}
      </div>
      {owner && (
        <div className="mb-3 empty:hidden">
          <SyncRemoteDecksResult sync={sync} />
        </div>
      )}
      {player.decks.length === 0 && <p className="text-base-content/60">No decks recorded yet.</p>}
      {active.length === 0 && retired.length > 0 && (
        <p className="text-base-content/60">Every deck is retired.</p>
      )}
      <DeckGrid decks={active} />
      {retired.length > 0 && (
        <details className="collapse-arrow border-base-300 bg-base-200/60 collapse mt-4 border">
          <summary className="collapse-title flex items-center gap-2 font-bold">
            <Archive className="size-4" /> Retired decks
            <span className="badge badge-sm badge-neutral">{retired.length}</span>
          </summary>
          <div className="collapse-content">
            <p className="text-base-content/60 mb-3 text-sm">
              Shelved for now; their games still count.
            </p>
            <DeckGrid decks={retired} />
          </div>
        </details>
      )}
    </section>
  )
}

function DeckGrid({ decks }: { decks: DeckSummary[] }) {
  if (decks.length === 0) return null
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {decks.map((deck) => (
        <Link
          key={deck.id}
          to="/decks/$deckId"
          params={{ deckId: String(deck.id) }}
          className="card group border-base-300 bg-base-200 hover:border-primary/40 relative overflow-hidden border transition-all hover:-translate-y-0.5 hover:shadow-xl"
        >
          <CardArtBackground imageUrl={deck.commander_art_crop_url} interactive />
          <div className="card-body text-base-content relative z-10 p-4">
            <span className="flex items-center justify-between gap-2">
              <strong>{deck.name}</strong>
              <ColorIdentity colors={deck.color_identity} />
            </span>
            <span className="text-base-content/85 text-sm">
              <DeckCommanders deck={deck} />
            </span>
            {deck.decklist_url && (
              <span className="text-base-content/70 inline-flex items-center gap-1 text-xs">
                <ExternalLink className="size-3" />
                {deckHostLabel(deck.decklist_source)}
              </span>
            )}
          </div>
        </Link>
      ))}
    </div>
  )
}

function deckHostLabel(source: string | null) {
  return source && source in remoteDeckSourceLabels
    ? remoteDeckSourceLabels[source as RemoteDeckSource]
    : "Deck list"
}

/**
 * Administrators fold duplicate players (an imported guest and the same person's
 * account, say) into one. Everything moves to the chosen player; this one goes away.
 */
function MergePlayer({ player }: { player: PlayerDetail }) {
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
    mutationFn: (target: PlayerSummary) => mergePlayers(player.id, target.id),
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
        {merge.error && !isSudoRequired(merge.error) && (
          <p className="text-error text-sm">
            {errorMessage(merge.error, "merge") ?? errorMessage(merge.error)}
          </p>
        )}
        <SudoPrompt
          error={merge.error}
          onSuccess={() => merge.variables && merge.mutate(merge.variables)}
        />
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
