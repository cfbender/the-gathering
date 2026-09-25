/**
 * Community names for canonical WUBRG identities, keyed as the games filter expects them
 * ("C" is colorless). Mirrors `TheGathering.Games.ColorIdentity`; keep both in sync.
 */
export const COLOR_IDENTITIES = [
  ["W", "Mono-White"],
  ["U", "Mono-Blue"],
  ["B", "Mono-Black"],
  ["R", "Mono-Red"],
  ["G", "Mono-Green"],
  ["WU", "Azorius"],
  ["WB", "Orzhov"],
  ["WR", "Boros"],
  ["WG", "Selesnya"],
  ["UB", "Dimir"],
  ["UR", "Izzet"],
  ["UG", "Simic"],
  ["BR", "Rakdos"],
  ["BG", "Golgari"],
  ["RG", "Gruul"],
  ["WUB", "Esper"],
  ["WUR", "Jeskai"],
  ["WUG", "Bant"],
  ["WBR", "Mardu"],
  ["WBG", "Abzan"],
  ["WRG", "Naya"],
  ["UBR", "Grixis"],
  ["UBG", "Sultai"],
  ["URG", "Temur"],
  ["BRG", "Jund"],
  ["WUBR", "Yore"],
  ["WUBG", "Witch"],
  ["WURG", "Ink"],
  ["WBRG", "Dune"],
  ["UBRG", "Glint"],
  ["WUBRG", "Five-Color"],
  ["C", "Colorless"],
] as const

export const COLOR_NAMES = {
  W: "White",
  U: "Blue",
  B: "Black",
  R: "Red",
  G: "Green",
} as const

/** Stats identify colorless decks by the empty identity; the games filter spells it "C". */
export function identityFilterValue(identity: string | number) {
  return String(identity) === "" ? "C" : String(identity)
}

export function colorIdentityName(identity: string) {
  return COLOR_IDENTITIES.find(([key]) => key === identity)?.[1] ?? identity
}
