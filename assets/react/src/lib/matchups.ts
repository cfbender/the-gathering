import { LEADERBOARD_MIN_GAMES, type MatchupRow, type NamedRecordRow } from "@/lib/stats"

export const MATCHUP_PLAYER_CAP = 8

export interface MatchupGrid {
  players: NamedRecordRow[]
  cells: (MatchupRow | null)[][]
}

/** Builds an ordered-pair matrix while preserving the leaderboard's player order. */
export function matchupGrid(players: NamedRecordRow[], matchups: MatchupRow[]): MatchupGrid {
  const included = players
    .filter((player) => player.games >= LEADERBOARD_MIN_GAMES)
    .slice(0, MATCHUP_PLAYER_CAP)
  const byPair = new Map(matchups.map((row) => [`${row.id}:${row.opponent_id}`, row]))

  return {
    players: included,
    cells: included.map((player) =>
      included.map((opponent) => byPair.get(`${player.id}:${opponent.id}`) ?? null),
    ),
  }
}
