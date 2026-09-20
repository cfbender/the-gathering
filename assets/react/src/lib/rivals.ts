interface NamedRow {
  name: string
}

interface PlayerRivalRow extends NamedRow {
  games: number
  wins: number
  losses: number
}

interface CommanderRivalRow extends NamedRow {
  faced: number
  beat_me: number
  beaten: number
}

function byCount<T extends NamedRow>(
  rows: T[],
  count: (row: T) => number,
  appearances: (row: T) => number,
): T | undefined {
  return [...rows].sort(
    (a, b) =>
      count(b) - count(a) ||
      appearances(b) - appearances(a) ||
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  )[0]
}

export function nemesisPlayer<T extends PlayerRivalRow>(rows: T[]): T | undefined {
  return byCount(
    rows,
    (row) => row.losses,
    (row) => row.games,
  )
}

export function favoriteVictim<T extends PlayerRivalRow>(rows: T[]): T | undefined {
  return byCount(
    rows,
    (row) => row.wins,
    (row) => row.games,
  )
}

export function nemesisCommander<T extends CommanderRivalRow>(rows: T[]): T | undefined {
  return byCount(
    rows,
    (row) => row.beat_me,
    (row) => row.faced,
  )
}

export function favoritePrey<T extends CommanderRivalRow>(rows: T[]): T | undefined {
  return byCount(
    rows,
    (row) => row.beaten,
    (row) => row.faced,
  )
}
