import { COLOR_NAMES, colorIdentityName } from "@/lib/color-identities"

/**
 * Filters for the games list, kept in the URL so other pages can deep-link to a
 * filtered view (for example a player's games with one commander, or Golgari wins).
 */
const numericKeys = [
  "player_id",
  "winner_id",
  "opponent_id",
  "winner_seat",
  "player_count",
  "min_turns",
  "max_turns",
  "min_duration",
  "max_duration",
] as const
/** Zero-based clock filters in the viewer's local time: Sunday is weekday 0, midnight hour 0. */
const clockKeys = { weekday: 6, hour: 23 } as const
const textKeys = [
  "commander",
  "colors",
  "winner_colors",
  "color",
  "winner_color",
  "win_condition",
  "player_result",
  "date_from",
  "date_to",
] as const

type NumericKey = (typeof numericKeys)[number] | keyof typeof clockKeys
type TextKey = (typeof textKeys)[number]

export const gameFilterKeys = [
  ...numericKeys,
  ...(Object.keys(clockKeys) as (keyof typeof clockKeys)[]),
  ...textKeys,
] as const

export type GameFilterKey = (typeof gameFilterKeys)[number]

/** Every filter as a string, "" when unset; the shape the form controls bind to. */
export type GameFilters = Record<GameFilterKey, string>

/**
 * Only the set filters plus the page, which is what the URL carries. Numeric filters
 * are numbers so the router serializes them as `player_id=9` rather than a quoted string.
 */
export type GamesSearch = Partial<Record<NumericKey, number>> &
  Partial<Record<TextKey, string>> & { page?: number }

/** Coerce raw URL search params: unknown keys and empty or invalid values are dropped. */
export function parseGamesSearch(search: Record<string, unknown>): GamesSearch {
  const result: GamesSearch = {}
  for (const key of numericKeys) {
    const value = integer(search[key])
    if (value !== undefined && value > 0) result[key] = value
  }
  for (const [key, max] of Object.entries(clockKeys) as [keyof typeof clockKeys, number][]) {
    const value = integer(search[key])
    if (value !== undefined && value >= 0 && value <= max) result[key] = value
  }
  for (const key of textKeys) {
    const value = search[key]
    if (typeof value === "string" && value.trim() !== "") result[key] = value.trim()
  }
  const page = integer(search.page)
  if (page !== undefined && page > 1) result.page = page
  return result
}

function integer(value: unknown) {
  if (typeof value !== "number" && typeof value !== "string") return undefined
  if (typeof value === "string" && value.trim() === "") return undefined
  const number = Number(value)
  return Number.isInteger(number) ? number : undefined
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

/**
 * Where a stats view's click-through links land: the first day its figures cover and,
 * on a player profile, that player.
 */
export interface GamesLinkScope {
  date_from?: string
  player_id?: number
}

/** Link options for the games list filtered by `search`, limited to the scope's date range. */
export function gamesLink(scope: GamesLinkScope, search: Record<string, unknown>) {
  return {
    to: "/games",
    search: parseGamesSearch({ date_from: scope.date_from, ...search }),
  } as const
}

/**
 * Narrows a scope to the administrator's detailed-stats cutoff, for figures (seats,
 * timings) that only count games from that date on. ISO dates compare as strings.
 */
export function detailedScope(scope: GamesLinkScope, detailedFrom: string | null) {
  if (!detailedFrom || (scope.date_from && scope.date_from >= detailedFrom)) return scope
  return { ...scope, date_from: detailedFrom }
}

const weekdays = [
  "Sundays",
  "Mondays",
  "Tuesdays",
  "Wednesdays",
  "Thursdays",
  "Fridays",
  "Saturdays",
]
const resultVerbs: Record<string, string> = { win: "won", loss: "lost", draw: "drew" }

export function hourLabel(hour: number) {
  if (hour === 0) return "12am"
  if (hour === 12) return "12pm"
  return hour < 12 ? `${hour}am` : `${hour - 12}pm`
}

/**
 * One Colors control covers four URL keys: an exact identity ("BG", "C") or a single color
 * (`has:B`), in any seat or only the winner's. `value` is "" when no color filter is set.
 */
export interface ColorChoice {
  value: string
  won: boolean
}

export function colorChoice(search: GamesSearch): ColorChoice {
  if (search.winner_colors) return { value: search.winner_colors, won: true }
  if (search.winner_color) return { value: `has:${search.winner_color}`, won: true }
  if (search.colors) return { value: search.colors, won: false }
  if (search.color) return { value: `has:${search.color}`, won: false }
  return { value: "", won: false }
}

/** The form patch for a Colors choice: sets its key and clears the other three. */
export function colorChoicePatch({ value, won }: ColorChoice): Partial<GameFilters> {
  const patch: Partial<GameFilters> = { colors: "", winner_colors: "", color: "", winner_color: "" }
  if (!value) return patch
  if (value.startsWith("has:"))
    return { ...patch, [won ? "winner_color" : "color"]: value.slice("has:".length) }
  return { ...patch, [won ? "winner_colors" : "colors"]: value }
}

/** Filters under the games page's "More filters" disclosure; it opens when any is set. */
export const moreFilterKeys = [
  "opponent_id",
  "player_result",
  "winner_seat",
  "weekday",
  "hour",
] as const satisfies readonly GameFilterKey[]

export const WEEKDAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
] as const

/** Display order for the active-filter chips. */
const chipOrder = [
  "player_id",
  "player_result",
  "winner_id",
  "opponent_id",
  "commander",
  "colors",
  "winner_colors",
  "color",
  "winner_color",
  "win_condition",
  "winner_seat",
  "player_count",
  "min_turns",
  "max_turns",
  "min_duration",
  "max_duration",
  "date_from",
  "date_to",
  "weekday",
  "hour",
] as const satisfies readonly GameFilterKey[]

/** A removable, human-readable chip for every active filter, so the page states what it shows. */
export function gameFilterChips(
  search: GamesSearch,
  names: {
    player: (id: number) => string | undefined
    winCondition: (condition: string) => string
  },
): { key: GameFilterKey; label: string }[] {
  const player = (id: number) => names.player(id) ?? `player #${id}`
  const label = (key: (typeof chipOrder)[number]): string | undefined => {
    const value = search[key]
    if (value === undefined) return undefined
    switch (key) {
      case "player_id":
        return `Player: ${player(Number(value))}`
      case "player_result": {
        const verb = resultVerbs[String(value)]
        if (!verb) return undefined
        return `${search.player_id === undefined ? "Player" : player(search.player_id)} ${verb}`
      }
      case "winner_id":
        return `Winner: ${player(Number(value))}`
      case "opponent_id":
        return `With ${player(Number(value))}`
      case "commander":
        return `Commander: ${value}`
      case "colors":
        return `Played ${colorIdentityName(String(value))}`
      case "winner_colors":
        return `Won with ${colorIdentityName(String(value))}`
      case "color":
        return `Includes ${colorName(String(value))}`
      case "winner_color":
        return `Won with ${colorName(String(value))}`
      case "win_condition":
        return `Ended by ${names.winCondition(String(value))}`
      case "winner_seat":
        return `Won from seat ${value}`
      case "player_count":
        return `${value} players`
      case "min_turns":
        return `At least ${value} turns`
      case "max_turns":
        return `At most ${value} turns`
      case "min_duration":
        return `At least ${value} min`
      case "max_duration":
        return `At most ${value} min`
      case "date_from":
        return `From ${formatFilterDate(String(value))}`
      case "date_to":
        return `Through ${formatFilterDate(String(value))}`
      case "weekday":
        return `On ${weekdays[Number(value)]}`
      case "hour":
        return `Played ${hourLabel(Number(value))}–${hourLabel((Number(value) + 1) % 24)}`
    }
  }
  return chipOrder.flatMap((key) => {
    const text = label(key)
    return text ? [{ key, label: text }] : []
  })
}

/** Filter dates are local calendar days; parse them at local midnight so they do not shift. */
function formatFilterDate(value: string) {
  const date = new Date(`${value}T00:00:00`)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date)
}

function colorName(color: string) {
  return COLOR_NAMES[color as keyof typeof COLOR_NAMES] ?? color
}
