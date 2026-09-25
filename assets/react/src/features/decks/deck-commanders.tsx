import type { ReactNode } from "react"
import { CardHover } from "@/components/card-hover"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { cn } from "@/lib/cn"
import type { DeckSummary } from "./decks"

type CommanderFields = Pick<
  DeckSummary,
  "commander_name" | "partner_name" | "commander_game_changer" | "partner_game_changer"
> &
  Partial<
    Pick<
      DeckSummary,
      | "commander_image_url"
      | "commander_art_crop_url"
      | "partner_image_url"
      | "partner_art_crop_url"
    >
  >

/**
 * Commander and partner names. With `hover`, each name previews its full card; leave it off
 * where a parent already wraps the whole pairing in a `CommanderHover`.
 */
export function DeckCommanders({
  deck,
  compact = false,
  hover = false,
}: {
  compact?: boolean
  hover?: boolean
  deck: CommanderFields
}) {
  const nameClass = compact ? "min-w-0 truncate" : undefined
  return (
    <span
      className={cn(
        "inline-flex min-w-0 max-w-full items-center gap-x-1.5 gap-y-1",
        !compact && "flex-wrap",
      )}
    >
      <CommanderName
        hover={hover}
        name={deck.commander_name}
        gameChanger={deck.commander_game_changer}
        imageUrl={deck.commander_image_url}
        artCropUrl={deck.commander_art_crop_url}
      >
        <span className={nameClass}>{deck.commander_name}</span>
      </CommanderName>
      <GameChangerBadge gameChanger={deck.commander_game_changer} compact={compact} />
      {deck.partner_name && (
        <>
          <span aria-hidden="true">/</span>
          <CommanderName
            hover={hover}
            name={deck.partner_name}
            gameChanger={deck.partner_game_changer}
            imageUrl={deck.partner_image_url}
            artCropUrl={deck.partner_art_crop_url}
          >
            <span className={nameClass}>{deck.partner_name}</span>
          </CommanderName>
          <GameChangerBadge gameChanger={deck.partner_game_changer} compact={compact} />
        </>
      )}
    </span>
  )
}

function CommanderName({
  hover,
  name,
  gameChanger,
  imageUrl,
  artCropUrl,
  children,
}: {
  hover: boolean
  name: string
  gameChanger?: boolean
  imageUrl?: string | null
  artCropUrl?: string | null
  children: ReactNode
}) {
  if (!hover) return children
  return (
    <CardHover
      id={null}
      name={name}
      gameChanger={gameChanger}
      imageUrl={imageUrl}
      artCropUrl={artCropUrl}
    >
      {children}
    </CardHover>
  )
}
