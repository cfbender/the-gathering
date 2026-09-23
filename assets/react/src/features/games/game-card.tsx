import { Link } from "@tanstack/react-router"
import { Trophy, UsersRound } from "lucide-react"
import { ColorIdentity } from "@/components/mana-symbols"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { formatDate, winConditionLabel, type Game, type Seat } from "./games"

function TablePlayer({ seat }: { seat: Seat }) {
  return (
    <li className="flex min-w-0 flex-col items-center gap-2 text-center">
      <div className="border-base-content/15 bg-base-300 relative flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-full border sm:size-14">
        <UsersRound aria-hidden="true" className="text-base-content/40 size-5" />
        {seat.deck?.commander_art_crop_url && (
          <img
            src={seat.deck.commander_art_crop_url}
            alt=""
            loading="lazy"
            decoding="async"
            className="absolute inset-0 size-full object-cover"
            onError={(event) => (event.currentTarget.hidden = true)}
          />
        )}
      </div>
      <div className="w-full min-w-0">
        <div className="flex items-center justify-center gap-1.5">
          <strong className="min-w-0 truncate text-sm" title={seat.player.name}>
            {seat.player.name}
          </strong>
          {seat.deck && <ColorIdentity colors={seat.deck.color_identity} className="text-xs" />}
        </div>
        <p
          className="text-base-content/80 mt-0.5 text-xs break-words"
          title={seat.deck?.commander_name}
        >
          {seat.deck ? <DeckCommanders deck={seat.deck} /> : "Unknown commander"}
        </p>
      </div>
    </li>
  )
}

export function GameCard({ game }: { game: Game }) {
  const winner = game.seats.find((seat) => seat.result === "win")
  const table = winner ? game.seats.filter((seat) => seat.id !== winner.id) : game.seats

  return (
    <Link
      to="/games/$gameId"
      params={{ gameId: String(game.id) }}
      className="card border-base-300 bg-base-200 hover:border-primary focus-visible:outline-primary overflow-hidden border transition-colors focus-visible:outline-2 focus-visible:outline-offset-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 px-5 py-4">
        <h2 className="text-sm font-bold">
          <time dateTime={game.played_at}>{formatDate(game.played_at)}</time>
        </h2>
        <span className="text-base-content/65 flex items-center gap-1.5 text-xs">
          <UsersRound aria-hidden="true" className="size-3.5" />
          {game.seats.length} players
        </span>
      </div>

      {winner ? (
        <section
          aria-label="Winner"
          className="relative isolate mx-5 flex min-h-60 flex-col justify-end overflow-hidden rounded-xl bg-stone-900 p-5 text-white ring-1 ring-white/15"
        >
          {winner.deck?.commander_art_crop_url && (
            <img
              src={winner.deck.commander_art_crop_url}
              alt=""
              loading="lazy"
              decoding="async"
              className="absolute inset-0 -z-20 size-full object-cover object-[center_35%]"
              onError={(event) => (event.currentTarget.hidden = true)}
            />
          )}
          <div
            aria-hidden="true"
            className="absolute inset-0 -z-10 bg-gradient-to-t from-black/95 via-black/30 to-transparent"
          />
          {game.win_condition && (
            <div className="mb-auto pb-4">
              <span
                aria-label={`Win condition: ${winConditionLabel(game.win_condition)}`}
                className="inline-flex max-w-full rounded-full border border-white/25 bg-black/65 px-3 py-1 text-xs font-semibold text-white backdrop-blur-sm"
              >
                {winConditionLabel(game.win_condition)}
              </span>
            </div>
          )}
          <p className="mb-2 flex items-center gap-1.5 text-xs font-bold tracking-widest text-amber-200 uppercase">
            <Trophy aria-hidden="true" className="size-4" /> Winner
          </p>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <p className="min-w-0 text-3xl leading-tight font-bold break-words">
              {winner.player.name}
            </p>
            {winner.deck && (
              <ColorIdentity colors={winner.deck.color_identity} className="text-sm" />
            )}
          </div>
          <p className="mt-1 text-sm break-words">
            {winner.deck ? <DeckCommanders deck={winner.deck} /> : "Unknown commander"}
          </p>
          <p className="mt-1 text-xs break-words text-white/75">
            {winner.deck?.name ?? "Unknown deck"}
          </p>
        </section>
      ) : (
        <section
          aria-label="Draw"
          className="from-info/15 to-base-200/30 flex min-h-60 flex-col items-center justify-center gap-2 bg-gradient-to-br p-5 text-center"
        >
          <UsersRound aria-hidden="true" className="text-info mb-1 size-8" />
          <p className="text-2xl font-bold">Draw</p>
          <p className="text-base-content/65 text-sm">No winner at this table</p>
        </section>
      )}

      <div className="p-5">
        <p className="text-base-content/55 mb-4 text-[0.625rem] font-bold tracking-widest uppercase">
          {winner ? "The rest of the table" : "At the table"}
        </p>
        <ul
          aria-label={winner ? "Other players" : "Players"}
          className="grid grid-cols-[repeat(auto-fit,minmax(5rem,1fr))] gap-x-3 gap-y-5"
        >
          {table.map((seat) => (
            <TablePlayer key={seat.id} seat={seat} />
          ))}
        </ul>
      </div>
    </Link>
  )
}
