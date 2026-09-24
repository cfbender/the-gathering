import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { Clock, Pencil, Trophy } from "lucide-react"
import { canManageGame, formatDate, getGame, winConditionLabel } from "@/features/games/games"
import { DeleteGame } from "@/features/games/delete-game"
import { CardArtBackground } from "@/components/card-art-background"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { useCurrentUser } from "@/lib/auth"
import { formatLabel } from "@/features/games/game-format"

export const Route = createFileRoute("/games/$gameId")({ component: GameDetailPage })

function GameDetailPage() {
  const { gameId } = Route.useParams()
  const query = useQuery({ queryKey: ["games", gameId], queryFn: () => getGame(gameId) })
  const viewer = useCurrentUser()
  if (query.isPending)
    return (
      <div className="flex justify-center py-16">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  if (query.isError) return <div className="alert alert-error">Game not found.</div>
  const game = query.data
  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-6">
      <PageHeader
        eyebrow={`Game #${game.id}`}
        title={formatDate(game.played_at)}
        actions={
          canManageGame(viewer.data, game) && (
            <div className="flex flex-wrap justify-end gap-2">
              <Link to="/games/$gameId/edit" params={{ gameId }} className="btn btn-outline">
                <Pencil className="size-4" /> Edit
              </Link>
              <DeleteGame game={game} />
            </div>
          )
        }
      >
        {game.format !== "commander" && (
          <span className="badge badge-outline mt-3">{formatLabel(game.format)}</span>
        )}
        {(game.turns || game.duration_minutes) && (
          <p className="text-base-content/60 mt-3 flex gap-4 text-sm">
            {game.turns && <span>{game.turns} turns</span>}
            {game.duration_minutes && (
              <span className="flex items-center gap-1">
                <Clock className="size-4" />
                {game.duration_minutes} minutes
              </span>
            )}
          </p>
        )}
      </PageHeader>
      <section className="grid gap-4 sm:grid-cols-2">
        {game.seats.map((seat) => (
          <article
            key={seat.id}
            className={`card relative overflow-hidden border ${seat.result === "win" ? "border-success bg-success/10" : "border-base-300 bg-base-200"}`}
          >
            <CardArtBackground imageUrl={seat.deck?.commander_art_crop_url} />
            <div className="card-body text-base-content relative z-10 gap-2 p-5">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="badge badge-neutral">Seat {seat.seat}</span>
                  <span className="badge badge-neutral">
                    {seat.kills === null
                      ? "Kills unknown"
                      : `${seat.kills} ${seat.kills === 1 ? "kill" : "kills"}`}
                  </span>
                </div>
                {seat.result === "win" && (
                  <span className="bg-base-100 text-accent flex items-center gap-1 rounded-full px-2 py-1 text-sm font-bold">
                    <Trophy className="size-4" /> Winner
                  </span>
                )}
                {seat.result === "draw" && <span className="badge badge-info">Draw</span>}
              </div>
              <Link
                to="/players/$playerId"
                params={{ playerId: String(seat.player.id) }}
                className="link-hover mt-2 text-xl font-bold"
              >
                {seat.player.name}
              </Link>
              {seat.deck ? (
                <Link
                  to="/decks/$deckId"
                  params={{ deckId: String(seat.deck.id) }}
                  className="link-hover"
                >
                  <span className="flex items-center justify-between gap-2">
                    <strong>{seat.deck.name}</strong>
                    <ColorIdentity colors={seat.deck.color_identity} />
                  </span>
                  <span className="text-base-content/85 block text-sm">
                    <DeckCommanders deck={seat.deck} />
                  </span>
                </Link>
              ) : (
                <p className="text-base-content/50">No deck recorded</p>
              )}
              {seat.mvp_card_name && (
                <div className="border-accent/40 relative mt-2 overflow-hidden rounded-lg border px-3 py-2 text-sm">
                  <CardArtBackground imageUrl={seat.mvp_art_crop_url} />
                  <p className="text-accent relative z-10">
                    <strong>MVP:</strong> {seat.mvp_card_name}{" "}
                    <GameChangerBadge gameChanger={seat.mvp_game_changer} />
                  </p>
                </div>
              )}
            </div>
          </article>
        ))}
      </section>
      {game.win_condition && (
        <section className="card border-base-300 bg-base-200 border">
          <div className="card-body">
            <h2 className="font-bold">Win condition</h2>
            <p>{winConditionLabel(game.win_condition)}</p>
          </div>
        </section>
      )}
      {game.notes && (
        <section className="card border-base-300 bg-base-200 border">
          <div className="card-body">
            <h2 className="font-bold">Table notes</h2>
            <p className="whitespace-pre-wrap">{game.notes}</p>
          </div>
        </section>
      )}
    </div>
  )
}
