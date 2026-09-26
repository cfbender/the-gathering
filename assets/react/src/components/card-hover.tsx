import { Anchor } from "@radix-ui/react-popover"
import { useState, type ReactNode } from "react"
import { CardImage } from "@/components/card-image"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { Popover, PopoverContent } from "@/components/ui/popover"
import type { DeckSummary } from "@/features/decks/decks"
import { printingPrices, usePrintingDetails } from "@/features/webcam-table/card-details"
import { cn } from "@/lib/cn"

/** Shared, non-focusing hover preview. Portalled so rail and list overflow cannot clip it. */
export function CardHover({
  id,
  name,
  imageUrl,
  artCropUrl,
  gameChanger,
  children,
}: {
  id: string | null
  name: string
  imageUrl?: string | null
  artCropUrl?: string | null
  gameChanger?: boolean
  children: ReactNode
}) {
  return (
    <HoverPopover
      label={`${name} image preview`}
      preview={
        <CardHoverPreview
          id={id}
          name={name}
          imageUrl={imageUrl}
          artCropUrl={artCropUrl}
          gameChanger={gameChanger}
        />
      }
    >
      {children}
    </HoverPopover>
  )
}

function HoverPopover({
  label,
  className,
  preview,
  children,
}: {
  label: string
  className?: string
  preview: ReactNode
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Anchor asChild>
        <span
          className="inline-flex min-w-0 max-w-full"
          onMouseEnter={() => setOpen(true)}
          onMouseLeave={() => setOpen(false)}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onClick={() => setOpen(false)}
        >
          {children}
        </span>
      </Anchor>
      <PopoverContent
        side="right"
        className={cn(
          "pointer-events-none w-64 max-w-[45vw] border-white/15 bg-black p-2 text-white",
          className,
        )}
        aria-label={label}
        onOpenAutoFocus={(event) => event.preventDefault()}
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        {preview}
      </PopoverContent>
    </Popover>
  )
}

/** Mounted only while the popover is open, so closed hovers never touch the query cache. */
function CardHoverPreview({
  id,
  name,
  imageUrl,
  artCropUrl,
  gameChanger,
}: {
  id: string | null
  name: string
  imageUrl?: string | null
  artCropUrl?: string | null
  gameChanger?: boolean
}) {
  const details = usePrintingDetails(id)
  return (
    <>
      <CardImage
        imageUris={
          details.data?.image_uris ?? {
            normal: imageUrl ?? undefined,
            art_crop: artCropUrl ?? undefined,
          }
        }
        name={name}
        variant={id || imageUrl ? "card" : "art"}
        className="w-full"
      />
      <p className="mt-1 text-center text-xs">{name}</p>
      <GameChangerBadge gameChanger={details.data?.game_changer ?? gameChanger} />
      {details.data && (
        <p className="mt-1 text-center text-xs text-white/70">
          {printingPrices(details.data.prices)}
        </p>
      )}
      {details.isPending && id && (
        <p className="text-center text-xs text-white/60">Loading card…</p>
      )}
    </>
  )
}

type CommanderHoverDeck = Pick<
  DeckSummary,
  "commander_name" | "commander_image_url" | "commander_art_crop_url" | "commander_game_changer"
> &
  Partial<
    Pick<
      DeckSummary,
      "partner_name" | "partner_image_url" | "partner_art_crop_url" | "partner_game_changer"
    >
  >

/** Previews a deck's commander, or both cards side by side for a partner pairing. */
export function CommanderHover({
  deck,
  children,
}: {
  deck: CommanderHoverDeck | undefined
  children: ReactNode
}) {
  if (!deck) return children
  const commander = (
    <CardHoverPreview
      id={null}
      name={deck.commander_name}
      gameChanger={deck.commander_game_changer}
      imageUrl={deck.commander_image_url}
      artCropUrl={deck.commander_art_crop_url}
    />
  )
  if (!deck.partner_name) {
    return (
      <HoverPopover label={`${deck.commander_name} image preview`} preview={commander}>
        {children}
      </HoverPopover>
    )
  }
  return (
    <HoverPopover
      label={`${deck.commander_name} and ${deck.partner_name} image preview`}
      className="w-[32rem] max-w-[80vw]"
      preview={
        <div className="grid grid-cols-2 gap-2">
          <div>{commander}</div>
          <div>
            <CardHoverPreview
              id={null}
              name={deck.partner_name}
              gameChanger={deck.partner_game_changer}
              imageUrl={deck.partner_image_url}
              artCropUrl={deck.partner_art_crop_url}
            />
          </div>
        </div>
      }
    >
      {children}
    </HoverPopover>
  )
}
