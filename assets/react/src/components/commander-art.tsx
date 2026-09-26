import type { CSSProperties } from "react"
import { cn } from "@/lib/cn"

// The split runs 30° off vertical through the centre on any aspect ratio: each side
// reaches `tan(30°) / 2` of the height past the middle. `cqh` resolves against the
// wrapper, which is a size container, so round portraits and wide banners share one slant.
const REACH = "28.87cqh"
const SHIFT = "57.74cqh"
const GAP = "1.5px"

const leftStyle: CSSProperties = {
  left: 0,
  width: `calc(50% + ${REACH})`,
  clipPath: `polygon(0 0, calc(100% - ${GAP}) 0, calc(100% - ${SHIFT} - ${GAP}) 100%, 0 100%)`,
}

const rightStyle: CSSProperties = {
  right: 0,
  width: `calc(50% + ${REACH})`,
  clipPath: `polygon(calc(${SHIFT} + ${GAP}) 0, 100% 0, 100% 100%, ${GAP} 100%)`,
}

/**
 * Fills its positioned parent with a commander's art crop. With a partner, the two crops
 * share the frame, split along a diagonal; a missing crop leaves the other one whole.
 * Broken images hide so the parent's fallback shows through.
 */
export function CommanderArt({
  imageUrl,
  partnerImageUrl,
  className,
  imageClassName,
}: {
  imageUrl?: string | null
  partnerImageUrl?: string | null
  className?: string
  imageClassName?: string
}) {
  const lone = imageUrl || partnerImageUrl
  if (!lone) return null
  const split = imageUrl && partnerImageUrl

  return (
    <div
      className={cn("pointer-events-none absolute inset-0 overflow-hidden", className)}
      style={split ? { containerType: "size" } : undefined}
    >
      {split ? (
        <>
          <ArtImage src={imageUrl} style={leftStyle} className={imageClassName} />
          <ArtImage src={partnerImageUrl} style={rightStyle} className={imageClassName} />
        </>
      ) : (
        <ArtImage src={lone} className={cn("inset-x-0 w-full", imageClassName)} />
      )}
    </div>
  )
}

function ArtImage({
  src,
  style,
  className,
}: {
  src: string
  style?: CSSProperties
  className?: string
}) {
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      decoding="async"
      style={style}
      className={cn("absolute inset-y-0 h-full max-w-none object-cover", className)}
      onError={(event) => (event.currentTarget.hidden = true)}
    />
  )
}
