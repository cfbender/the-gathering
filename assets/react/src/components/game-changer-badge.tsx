import { Sparkles } from "lucide-react"

/** Missing flags (legacy snapshots or details still loading) are not a positive match. */
export function GameChangerBadge({
  gameChanger,
  compact = false,
}: {
  gameChanger?: boolean
  compact?: boolean
}) {
  if (!gameChanger) return null
  return (
    <span
      className="badge badge-warning badge-sm h-auto min-h-5 shrink-0 gap-1 whitespace-nowrap px-1.5 py-0.5 text-[0.65rem] font-semibold normal-case tracking-normal"
      title="On the Commander Brackets Game Changers list (Scryfall)"
    >
      <Sparkles className="size-3 shrink-0" aria-hidden="true" />
      <span className={compact ? "sr-only" : undefined}>Game Changer</span>
    </span>
  )
}
