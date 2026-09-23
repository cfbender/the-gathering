import { GameChangerBadge } from "@/components/game-changer-badge"
import { cn } from "@/lib/cn"
import type { DeckSummary } from "./decks"

export function DeckCommanders({
  deck,
  compact = false,
}: {
  compact?: boolean
  deck: Pick<
    DeckSummary,
    "commander_name" | "partner_name" | "commander_game_changer" | "partner_game_changer"
  >
}) {
  return (
    <span
      className={cn(
        "inline-flex min-w-0 max-w-full items-center gap-x-1.5 gap-y-1",
        !compact && "flex-wrap",
      )}
    >
      <span className={compact ? "min-w-0 truncate" : undefined}>{deck.commander_name}</span>
      <GameChangerBadge gameChanger={deck.commander_game_changer} compact={compact} />
      {deck.partner_name && (
        <>
          <span aria-hidden="true">/</span>
          <span className={compact ? "min-w-0 truncate" : undefined}>{deck.partner_name}</span>
          <GameChangerBadge gameChanger={deck.partner_game_changer} compact={compact} />
        </>
      )}
    </span>
  )
}
