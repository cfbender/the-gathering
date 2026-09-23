/** Identity tints sampled at Convoke's lightness: barely-there color over a near-black bar so
 * the white seat controls stay readable and the bar never competes with the video. */
const COLORS: Record<string, string> = {
  W: "#2b2818",
  U: "#0b1a2e",
  B: "#1a1421",
  R: "#290c0f",
  G: "#0a1c10",
}

const NEUTRAL = "#18181b"

export function commanderBackground(identity: string): string {
  const colors = Object.keys(COLORS).filter((color) => identity.includes(color))
  if (colors.length === 0) return NEUTRAL
  if (colors.length === 1) return COLORS[colors[0]!]!
  return `linear-gradient(90deg, ${colors.map((color) => COLORS[color]).join(", ")})`
}
