export const GAME_FORMATS = [
  { value: "commander", label: "Commander" },
  { value: "two_headed_giant", label: "2HG", name: "Two-Headed Giant Commander" },
  { value: "five_star", label: "Five Star" },
] as const

export type GameFormat = (typeof GAME_FORMATS)[number]["value"]

export function isGameFormat(value: string): value is GameFormat {
  return GAME_FORMATS.some((format) => format.value === value)
}

export function formatLabel(format: GameFormat) {
  return GAME_FORMATS.find((option) => option.value === format)?.label ?? "Commander"
}
