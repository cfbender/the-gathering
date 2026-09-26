import { CommanderArt } from "@/components/commander-art"
import { cn } from "@/lib/cn"

export function CardArtBackground({
  imageUrl,
  partnerImageUrl,
  interactive = false,
}: {
  imageUrl?: string | null
  /** A partner's crop splits the background diagonally with `imageUrl`. */
  partnerImageUrl?: string | null
  interactive?: boolean
}) {
  if (!imageUrl && !partnerImageUrl) return null

  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 overflow-hidden">
      <CommanderArt
        imageUrl={imageUrl}
        partnerImageUrl={partnerImageUrl}
        imageClassName={cn(
          "opacity-75 transition duration-300",
          interactive && "group-hover:opacity-45",
        )}
      />
      <div className="from-base-100/98 via-base-100/25 absolute inset-0 bg-gradient-to-br to-transparent" />
    </div>
  )
}
