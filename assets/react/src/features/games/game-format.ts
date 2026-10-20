/** Formats a game can be recorded in: Commander variants only. */
export const GAME_FORMATS = [
  { value: "commander", label: "Commander" },
  { value: "two_headed_giant", label: "2HG", name: "Two-Headed Giant Commander" },
  { value: "five_star", label: "Five Star" },
  { value: "duel_commander", label: "Duel Commander" },
] as const

/** Webcam-table modes: every recorded format plus one-on-one formats that are played but not
 * recorded. */
export const TABLE_MODES = [
  ...GAME_FORMATS,
  { value: "standard", label: "Standard" },
  { value: "pauper", label: "Pauper" },
  { value: "canadian_highlander", label: "Canadian Highlander" },
] as const

export type GameFormat = (typeof GAME_FORMATS)[number]["value"]
export type TableMode = (typeof TABLE_MODES)[number]["value"]

export function isGameFormat(value: string): value is GameFormat {
  return GAME_FORMATS.some((format) => format.value === value)
}

export function isTableMode(value: string): value is TableMode {
  return TABLE_MODES.some((mode) => mode.value === value)
}

/** One-on-one modes, which start at 20 life with exactly two players. */
export function isDuelMode(mode: TableMode) {
  return (
    mode === "duel_commander" ||
    mode === "standard" ||
    mode === "pauper" ||
    mode === "canadian_highlander"
  )
}

export function formatLabel(format: TableMode) {
  return TABLE_MODES.find((option) => option.value === format)?.label ?? "Commander"
}
