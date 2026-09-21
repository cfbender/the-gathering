import { Link } from "@tanstack/react-router"
import { Trophy, UsersRound } from "lucide-react"
import { ColorIdentity } from "@/components/mana-symbols"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { cn } from "@/lib/cn"
import { formatDate, type Game, type Seat } from "./games"

function PlayerPortrait({ seat }: { seat: Seat }) {
  const winner = seat.result === "win"
  const result = winner ? "Winner" : seat.result === "draw" ? "Draw" : "Loss"

  return (
    <li>
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={`${seat.player.name} — ${result}. View commander details`}
            className="hover:bg-base-content/5 focus-visible:outline-primary flex w-14 flex-col items-center gap-1 rounded-lg p-1 focus-visible:outline-2 focus-visible:outline-offset-2"
          >
            <span className="relative">
              <span
                className={cn(
                  "bg-base-300 border-base-content/20 relative flex size-10 items-center justify-center overflow-hidden rounded-full border-2",
                  winner && "border-accent",
                )}
              >
                <UsersRound aria-hidden="true" className="text-base-content/50 size-4" />
                {seat.deck?.commander_art_crop_url && (
                  <img
                    src={seat.deck.commander_art_crop_url}
                    alt=""
                    loading="lazy"
                    className="absolute inset-0 size-full object-cover"
                    onError={(event) => (event.currentTarget.hidden = true)}
                  />
                )}
              </span>
              {winner && (
                <span className="bg-accent text-accent-content absolute -top-1 -right-1 rounded-full p-0.5">
                  <Trophy aria-hidden="true" className="size-3" />
                </span>
              )}
            </span>
            <span className="w-full truncate text-center text-xs">{seat.player.name}</span>
          </button>
        </PopoverTrigger>
        <PopoverContent aria-label={`${seat.player.name}'s commander details`} className="w-64">
          <p className="font-bold break-words">{seat.player.name}</p>
          <p className="text-base-content/65 mb-3 text-xs">
            {result} · Seat {seat.seat}
          </p>
          <p className="text-sm font-semibold break-words">
            {seat.deck?.commander_name ?? "Unknown commander"}
          </p>
          {seat.deck?.partner_name && (
            <p className="text-sm break-words">{seat.deck.partner_name}</p>
          )}
          <p className="text-base-content/70 mt-1 text-xs break-words">
            {seat.deck?.name ?? "Unknown deck"}
          </p>
          {seat.deck && (
            <ColorIdentity colors={seat.deck.color_identity} className="mt-2 text-xs" />
          )}
        </PopoverContent>
      </Popover>
    </li>
  )
}

export function GameTable({ games }: { games: Game[] }) {
  return (
    <div className="card border-base-300 bg-base-200 min-w-0 overflow-hidden border">
      <div
        role="region"
        aria-label="Games table, scroll horizontally for more columns"
        tabIndex={0}
        className="focus-visible:outline-primary overflow-x-auto focus-visible:-outline-offset-2 focus-visible:outline-2"
      >
        <table className="table w-full min-w-168">
          <caption className="sr-only">
            Games — select a date to view the game or a player for commander details
          </caption>
          <thead>
            <tr>
              <th scope="col">Played</th>
              <th scope="col">Players</th>
              <th scope="col">Result</th>
              <th scope="col" className="text-right">
                Turns
              </th>
              <th scope="col" className="text-right">
                Duration
              </th>
            </tr>
          </thead>
          <tbody>
            {games.map((game) => {
              const winner = game.seats.find((seat) => seat.result === "win")
              return (
                <tr key={game.id} className="hover:bg-base-content/5">
                  <th scope="row" className="font-normal">
                    <Link
                      to="/games/$gameId"
                      params={{ gameId: String(game.id) }}
                      className="link link-hover text-primary focus-visible:outline-primary inline-block py-2 font-semibold focus-visible:outline-2 focus-visible:outline-offset-4"
                    >
                      <time dateTime={game.played_at}>{formatDate(game.played_at)}</time>
                      <span className="sr-only"> · Game {game.id}</span>
                    </Link>
                  </th>
                  <td className="py-2">
                    <ul aria-label={`${game.seats.length} players`} className="flex w-max gap-1">
                      {game.seats.map((seat) => (
                        <PlayerPortrait key={seat.id} seat={seat} />
                      ))}
                    </ul>
                  </td>
                  <td>
                    <span className="flex items-center gap-2">
                      {winner && (
                        <Trophy aria-hidden="true" className="text-accent size-4 shrink-0" />
                      )}
                      <span className="max-w-40 break-words">
                        {winner ? (
                          <>
                            <span className="sr-only">Winner: </span>
                            {winner.player.name}
                          </>
                        ) : (
                          "Draw"
                        )}
                      </span>
                    </span>
                  </td>
                  <td className="text-right tabular-nums">
                    {game.turns ?? <span aria-label="Not recorded">—</span>}
                  </td>
                  <td className="text-right whitespace-nowrap tabular-nums">
                    {game.duration_minutes === null ? (
                      <span aria-label="Not recorded">—</span>
                    ) : (
                      `${game.duration_minutes} min`
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}
