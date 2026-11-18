import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { ColorIdentity } from "@/components/mana-symbols"
import { Clock, Pencil, Trophy } from "lucide-react"
import { canManageGame, formatDate, getGame, winConditionLabel } from "@/features/games/games"
import { DeleteGame } from "@/features/games/delete-game"
import { MoveGame } from "@/features/games/move-game"
import { CardArtBackground } from "@/components/card-art-background"
import { CardHover } from "@/components/card-hover"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { useCurrentUser } from "@/lib/auth"
import { formatLabel } from "@/features/games/game-format"
import {
  SatisfactionBadge,
  SatisfactionPicker,
  SatisfactionPrompt,
  useGameRatingRequest,
  useRateGame,
} from "@/features/satisfaction/satisfaction"

export const Route = createFileRoute("/games/$gameId")({ component: GameDetailPage })

function GameDetailPage() {
  const { gameId } = Route.useParams()
  const query = useQuery({ queryKey: ["games", gameId], queryFn: () => getGame(gameId) })
  const viewer = useCurrentUser()
  const rate = useRateGame(Number(gameId))
  const [ratingRequested, clearRatingRequest] = useGameRatingRequest(Number(gameId))
  if (query.isPending)
    return (
      <div className="flex justify-center py-16">
        <span className="loading loading-spinner loading-lg" />
      </div>
    )
  if (query.isError) return <div className="alert alert-error">Game not found.</div>
  const game = query.data
  const mySeat = game.seats.find((seat) => viewer.data && seat.player.user_id === viewer.data.id)
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
        {(game.turns || game.duration_minutes || game.average_satisfaction !== null) && (
          <p className="text-base-content/60 mt-3 flex flex-wrap items-center gap-4 text-sm">
            {game.turns && <span>{game.turns} turns</span>}
            {game.duration_minutes && (
              <span className="flex items-center gap-1">
                <Clock className="size-4" />
                {game.duration_minutes} minutes
              </span>
            )}
            {game.average_satisfaction !== null && (
              <span className="flex items-center gap-1">
                Table vibe
                <SatisfactionBadge
                  average
                  value={game.average_satisfaction}
                  ratings={game.seats.filter((seat) => seat.satisfaction !== null).length}
                  className="text-base-content"
                />
              </span>
            )}
          </p>
        )}
      </PageHeader>
      <section className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        {game.seats.map((seat) => {
          const victims = game.seats.filter(
            (victim) => victim.eliminated_by_player_id === seat.player_id,
          )
          const unknown = seat.kills === null ? null : Math.max(0, seat.kills - victims.length)
          const victimNames = [
            ...victims.map((victim) => victim.player.name),
            ...(unknown ? [`${unknown} unknown`] : []),
            ...(victims.length && unknown === null ? ["other kills unknown"] : []),
          ].join(", ")
          return (
            <article
              key={seat.id}
              className={`card relative h-64 overflow-hidden border ${seat.result === "win" ? "border-success bg-success/10" : "border-base-300 bg-base-200"}`}
            >
              <CardArtBackground
                imageUrl={seat.deck?.commander_art_crop_url}
                partnerImageUrl={seat.deck?.partner_art_crop_url}
              />
              <div className="card-body text-base-content relative z-10 gap-2 p-5">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="badge badge-neutral">Seat {seat.seat}</span>
                    <span className="badge badge-neutral">
                      {seat.kills === null
                        ? "Kills unknown"
                        : `${seat.kills} ${seat.kills === 1 ? "kill" : "kills"}`}
                    </span>
                    {seat === mySeat ? (
                      <SatisfactionPicker
                        size="sm"
                        value={seat.satisfaction}
                        disabled={rate.isPending}
                        onRate={(value) => rate.mutate(value)}
                        className="bg-base-100/80 rounded-full px-1"
                      />
                    ) : (
                      <SatisfactionBadge
                        value={seat.satisfaction}
                        className="bg-base-100/80 rounded-full p-0.5"
                      />
                    )}
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
                {victimNames && (
                  <p
                    className="text-base-content/70 truncate text-sm"
                    title={`Killed: ${victimNames}`}
                  >
                    Killed: {victimNames}
                  </p>
                )}
                {seat.deck ? (
                  <Link
                    to="/decks/$deckId"
                    params={{ deckId: String(seat.deck.id) }}
                    className="link-hover min-w-0"
                  >
                    <span className="flex items-center justify-between gap-2">
                      <strong className="truncate" title={seat.deck.name}>
                        {seat.deck.name}
                      </strong>
                      <ColorIdentity colors={seat.deck.color_identity} />
                    </span>
                    <span className="text-base-content/85 flex text-sm">
                      <DeckCommanders deck={seat.deck} hover compact />
                    </span>
                  </Link>
                ) : (
                  <p className="text-base-content/50">No deck recorded</p>
                )}
                {seat.mvp_card_name && (
                  <div className="border-accent/40 relative mt-auto overflow-hidden rounded-lg border px-3 py-2 text-sm">
                    <CardArtBackground imageUrl={seat.mvp_art_crop_url} />
                    <p className="text-accent relative z-10 truncate">
                      <strong>MVP:</strong>{" "}
                      <CardHover
                        id={seat.mvp_image_url ? null : seat.mvp_card_id}
                        name={seat.mvp_card_name}
                        gameChanger={seat.mvp_game_changer}
                        imageUrl={seat.mvp_image_url}
                        artCropUrl={seat.mvp_art_crop_url}
                      >
                        <span className="decoration-accent/50 cursor-help underline decoration-dotted underline-offset-2">
                          {seat.mvp_card_name}
                        </span>
                      </CardHover>{" "}
                      <GameChangerBadge gameChanger={seat.mvp_game_changer} />
                    </p>
                  </div>
                )}
              </div>
            </article>
          )
        })}
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
      <MoveGame game={game} />
      {ratingRequested && mySeat?.satisfaction === null && (
        <SatisfactionPrompt
          onRate={(value) => {
            rate.mutate(value)
            clearRatingRequest()
          }}
          onDismiss={clearRatingRequest}
        />
      )}
    </div>
  )
}
