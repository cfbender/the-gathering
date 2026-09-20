import symbols from "@/assets/mana/symbols.json"
import { cn } from "@/lib/cn"

const symbolAssets = import.meta.glob("../assets/mana/*.svg", {
  eager: true,
  import: "default",
  query: "?url&no-inline",
}) as Record<string, string>

interface SymbolDetails {
  symbol: string
  filename: string
  english: string
}

const detailsBySymbol = new Map(
  (symbols as SymbolDetails[]).map((details) => [details.symbol.toUpperCase(), details]),
)

const colorNames: Record<string, string> = {
  W: "white",
  U: "blue",
  B: "black",
  R: "red",
  G: "green",
  C: "colorless",
}

const colorOrder = ["W", "U", "B", "R", "G"]

export type ManaCostPart =
  | { kind: "symbol"; token: string; label: string; src: string }
  | { kind: "text"; text: string }

function symbolDetails(symbol: string) {
  const token = symbol.startsWith("{") ? symbol : `{${symbol}}`
  const details = detailsBySymbol.get(token.toUpperCase())
  if (!details) return null
  const src = symbolAssets[`../assets/mana/${details.filename}`]
  if (!src) return null

  const key = token.slice(1, -1).toUpperCase()
  return {
    token,
    label: colorNames[key] ? `${colorNames[key]} mana` : details.english,
    src,
  }
}

export function parseManaCost(cost: string): ManaCostPart[] {
  return cost
    .split(/(\{[^}]+\})/g)
    .filter(Boolean)
    .map((part) => {
      if (!part.startsWith("{")) return { kind: "text", text: part }
      const details = symbolDetails(part)
      return details ? { kind: "symbol", ...details } : { kind: "text", text: part }
    })
}

export function ManaSymbol({ symbol, className }: { symbol: string; className?: string }) {
  const details = symbolDetails(symbol)
  if (!details) return <span className={className}>{symbol}</span>

  return (
    <img
      src={details.src}
      alt={details.label}
      title={details.label}
      className={cn(
        "mx-0.5 inline-block h-[1.15em] w-[1.15em] translate-y-[-0.08em] align-middle",
        className,
      )}
    />
  )
}

export function ManaCost({ cost, className }: { cost: string; className?: string }) {
  return (
    <span className={cn("inline-flex flex-wrap items-center gap-0.5", className)}>
      {parseManaCost(cost).map((part, index) =>
        part.kind === "symbol" ? (
          <ManaSymbol key={`${part.token}-${index}`} symbol={part.token} className="mx-0" />
        ) : (
          <span key={`${part.text}-${index}`}>{part.text}</span>
        ),
      )}
    </span>
  )
}

export function ColorIdentity({
  colors,
  className,
}: {
  colors: string | string[]
  className?: string
}) {
  const supplied = typeof colors === "string" ? colors.split("") : colors
  const ordered = colorOrder.filter((color) => supplied.includes(color))
  const displayed = supplied.includes("C") && ordered.length === 0 ? ["C"] : ordered
  const label = displayed.map((color) => colorNames[color]).join(", ")

  // An empty identity usually means "not recorded" (imported decks often have
  // none), so show nothing rather than claiming the deck is colorless. Pass "C"
  // explicitly for a genuinely colorless deck.
  if (displayed.length === 0) return null

  return (
    <span
      className={cn("inline-flex shrink-0 items-center gap-0.5", className)}
      aria-label={`Color identity: ${label}`}
    >
      {displayed.map((color) => (
        <ManaSymbol key={color} symbol={color} className="mx-0 translate-y-0" />
      ))}
    </span>
  )
}
