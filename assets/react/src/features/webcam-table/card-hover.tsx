import { Anchor } from "@radix-ui/react-popover"
import { useState, type ReactNode } from "react"
import { CardImage } from "@/components/card-image"
import { Popover, PopoverContent } from "@/components/ui/popover"
import type { DeckSummary } from "@/features/decks/decks"
import { printingPrices, usePrintingDetails } from "./card-details"

/** Shared, non-focusing hover preview. Portalled so rail and list overflow cannot clip it. */
export function CardHover({
  id,
  name,
  imageUrl,
  artCropUrl,
  children,
}: {
  id: string | null
  name: string
  imageUrl?: string | null
  artCropUrl?: string | null
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const details = usePrintingDetails(open ? id : null)
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
        className="pointer-events-none w-64 max-w-[45vw] border-white/15 bg-black p-2 text-white"
        aria-label={`${name} image preview`}
        onOpenAutoFocus={(event) => event.preventDefault()}
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
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
        {details.data && (
          <p className="mt-1 text-center text-xs text-white/70">
            {printingPrices(details.data.prices)}
          </p>
        )}
        {details.isPending && id && (
          <p className="text-center text-xs text-white/60">Loading card…</p>
        )}
      </PopoverContent>
    </Popover>
  )
}

export function CommanderHover({
  deck,
  children,
}: {
  deck:
    | Pick<DeckSummary, "commander_name" | "commander_image_url" | "commander_art_crop_url">
    | undefined
  children: ReactNode
}) {
  if (!deck) return children
  return (
    <CardHover
      id={null}
      name={deck.commander_name}
      imageUrl={deck.commander_image_url}
      artCropUrl={deck.commander_art_crop_url}
    >
      {children}
    </CardHover>
  )
}
