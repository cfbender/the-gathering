import { Link } from "@tanstack/react-router"
import { gamesLink, type GamesLinkScope } from "@/features/games/game-filters"
import { matchupGrid } from "@/lib/matchups"
import type { MatchupRow, NamedRecordRow } from "@/lib/stats"

export function MatchupHeatmap({
  players,
  matchups,
  games,
}: {
  players: NamedRecordRow[]
  matchups: MatchupRow[]
  /** Makes each cell a link to the games both players shared. */
  games?: GamesLinkScope
}) {
  const grid = matchupGrid(players, matchups)

  return (
    <section className="border-base-300 bg-base-200/60 rounded-xl border p-5">
      <p className="text-primary text-xs font-bold uppercase">Across the table</p>
      <h2 className="text-xl font-bold">Matchup heatmap</h2>
      <p className="text-base-content/55 mt-1 text-sm">
        Each row shows that player&apos;s win rate when sharing a pod with the column player.
        {games && " Click a cell to see those games."}
      </p>
      {grid.players.length < 2 ? (
        <p className="text-base-content/50 mt-5 text-sm">More shared games are needed.</p>
      ) : (
        <div className="mt-5 overflow-x-auto pb-2">
          <table className="mx-auto border-separate border-spacing-1 text-center text-xs">
            <thead>
              <tr>
                <th className="w-28" />
                {grid.players.map((player) => (
                  <th
                    key={player.id}
                    scope="col"
                    className="text-base-content/60 w-14 max-w-14 truncate px-1 pb-1 font-medium"
                    title={player.name}
                  >
                    {player.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {grid.players.map((player, rowIndex) => (
                <tr key={player.id}>
                  <th
                    scope="row"
                    className="text-base-content/70 w-28 max-w-28 truncate pr-2 text-right font-medium"
                    title={player.name}
                  >
                    {player.name}
                  </th>
                  {grid.players.map((opponent, columnIndex) => {
                    const matchup = grid.cells[rowIndex]?.[columnIndex]
                    const diagonal = player.id === opponent.id
                    return (
                      <td
                        key={opponent.id}
                        title={
                          matchup
                            ? `${player.name}: ${matchup.win_rate}% in ${matchup.games} shared games with ${opponent.name}`
                            : diagonal
                              ? undefined
                              : `${player.name} and ${opponent.name} have not shared a game`
                        }
                        className="border-base-300 h-14 w-14 min-w-14 rounded-md border tabular-nums"
                        style={
                          matchup
                            ? {
                                background: `color-mix(in oklab, var(--color-primary) ${15 + matchup.win_rate * 0.75}%, var(--color-base-200))`,
                              }
                            : undefined
                        }
                      >
                        {matchup && games ? (
                          <Link
                            {...gamesLink(games, {
                              player_id: player.id,
                              opponent_id: opponent.id,
                            })}
                            aria-label={`${player.name}: ${matchup.win_rate}% in ${matchup.games} shared games with ${opponent.name}. Show games`}
                            className="hover:ring-primary flex size-full flex-col items-center justify-center rounded-md font-bold hover:ring-2"
                          >
                            {matchup.win_rate}%
                            <small className="text-base-content/55 block font-normal">
                              {matchup.games}g
                            </small>
                          </Link>
                        ) : matchup ? (
                          <span className="font-bold">
                            {matchup.win_rate}%
                            <small className="text-base-content/55 block font-normal">
                              {matchup.games}g
                            </small>
                          </span>
                        ) : diagonal ? (
                          <span className="text-base-content/15">×</span>
                        ) : (
                          <span className="text-base-content/30">—</span>
                        )}
                      </td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
