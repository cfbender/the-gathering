import { Fragment, type ReactNode } from "react"
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

interface Commander {
  name: string
  gameChanger?: boolean
  imageUrl?: string | null
  artCropUrl?: string | null
}

function commandersOf(deck: CommanderFields): Commander[] {
  const commander = {
    name: deck.commander_name,
    gameChanger: deck.commander_game_changer,
    imageUrl: deck.commander_image_url,
    artCropUrl: deck.commander_art_crop_url,
  }
  if (!deck.partner_name) return [commander]
  return [
    commander,
    {
      name: deck.partner_name,
      gameChanger: deck.partner_game_changer,
      imageUrl: deck.partner_image_url,
      artCropUrl: deck.partner_art_crop_url,
    },
  ]
}

/**
 * Commander and partner names. With `hover`, each name previews its full card; leave it off
 * where a parent already wraps the whole pairing in a `CommanderHover`. `stacked` puts each
 * commander on its own centered line, for narrow columns where a wrapped "A / B" row reads badly.
 */
export function DeckCommanders({
  deck,
  compact = false,
  hover = false,
  stacked = false,
}: {
  compact?: boolean
  hover?: boolean
  stacked?: boolean
  deck: CommanderFields
}) {
  const commanders = commandersOf(deck)

  if (stacked) {
    return (
      <span className="flex min-w-0 flex-col items-center gap-0.5">
        {commanders.map((commander) => (
          <span
            key={commander.name}
            className="inline-flex max-w-full flex-wrap items-center justify-center gap-1"
          >
            <CommanderName hover={hover} commander={commander}>
              <span>{commander.name}</span>
            </CommanderName>
            <GameChangerBadge gameChanger={commander.gameChanger} compact />
          </span>
        ))}
      </span>
    )
  }

  const nameClass = compact ? "min-w-0 truncate" : undefined
  return (
    <span
      className={cn(
        "inline-flex min-w-0 max-w-full items-center gap-x-1.5 gap-y-1",
        !compact && "flex-wrap",
      )}
    >
      {commanders.map((commander, index) => (
        <Fragment key={commander.name}>
          {index > 0 && <span aria-hidden="true">/</span>}
          <CommanderName hover={hover} commander={commander}>
            <span className={nameClass}>{commander.name}</span>
          </CommanderName>
          <GameChangerBadge gameChanger={commander.gameChanger} compact={compact} />
        </Fragment>
      ))}
    </span>
  )
}

function CommanderName({
  hover,
  commander,
  children,
}: {
  hover: boolean
  commander: Commander
  children: ReactNode
}) {
  if (!hover) return children
  return (
    <CardHover
      id={null}
      name={commander.name}
      gameChanger={commander.gameChanger}
      imageUrl={commander.imageUrl}
      artCropUrl={commander.artCropUrl}
    >
      {children}
    </CardHover>
  )
}
