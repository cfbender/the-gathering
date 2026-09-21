/**
 * Filters for the games list, kept in the URL so other pages can deep-link to a
 * filtered view (for example a player's games with one commander).
 */
const numericKeys = [
  "player_id",
  "winner_id",
  "player_count",
  "min_turns",
  "max_turns",
  "min_duration",
  "max_duration",
] as const
const textKeys = ["commander", "date_from", "date_to"] as const

export const gameFilterKeys = [...numericKeys, ...textKeys] as const

export type GameFilterKey = (typeof gameFilterKeys)[number]

/** Every filter as a string, "" when unset; the shape the form controls bind to. */
export type GameFilters = Record<GameFilterKey, string>

/**
 * Only the set filters plus the page, which is what the URL carries. Numeric filters
 * are numbers so the router serializes them as `player_id=9` rather than a quoted string.
 */
export type GamesSearch = Partial<Record<(typeof numericKeys)[number], number>> &
  Partial<Record<(typeof textKeys)[number], string>> & { page?: number }

/** Coerce raw URL search params: unknown keys and empty or invalid values are dropped. */
export function parseGamesSearch(search: Record<string, unknown>): GamesSearch {
  const result: GamesSearch = {}
  for (const key of numericKeys) {
    const value = positiveInteger(search[key])
    if (value !== undefined) result[key] = value
  }
  for (const key of textKeys) {
    const value = search[key]
    if (typeof value === "string" && value.trim() !== "") result[key] = value.trim()
  }
  const page = positiveInteger(search.page)
  if (page !== undefined && page > 1) result.page = page
  return result
}

function positiveInteger(value: unknown) {
  if (typeof value !== "number" && typeof value !== "string") return undefined
  if (typeof value === "string" && value.trim() === "") return undefined
  const number = Number(value)
  return Number.isInteger(number) && number > 0 ? number : undefined
}

/** Fill in the unset filters so the form always has a string per field. */
export function toGameFilters(search: GamesSearch): GameFilters {
  return Object.fromEntries(
    gameFilterKeys.map((key) => [key, search[key] === undefined ? "" : String(search[key])]),
  ) as GameFilters
}

/** Apply a form change, dropping cleared filters and resetting to the first page. */
export function patchGamesSearch(search: GamesSearch, patch: Partial<GameFilters>): GamesSearch {
  return parseGamesSearch({ ...search, ...patch, page: undefined })
}

export function countActiveGameFilters(search: GamesSearch) {
  return gameFilterKeys.filter((key) => search[key] !== undefined).length
}
