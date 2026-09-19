import { ImageOff } from "lucide-react"
import { useEffect, useState } from "react"
import type { CardImageUris } from "@/lib/cards"
import { cn } from "@/lib/cn"

interface CardImageProps {
  imageUris: CardImageUris
  name: string
  variant?: "art" | "card"
  className?: string
}

export function CardImage({ imageUris, name, variant = "art", className }: CardImageProps) {
  const src = variant === "card" ? imageUris.normal : (imageUris.art_crop ?? imageUris.normal)
  const [failed, setFailed] = useState(false)

  useEffect(() => setFailed(false), [src])

  if (!src || failed) {
    return (
      <div
        role="img"
        aria-label={`No image available for ${name}`}
        className={cn(
          "bg-base-300 text-base-content/40 grid place-items-center overflow-hidden",
          variant === "card" ? "aspect-[5/7] rounded-xl" : "aspect-[4/3] rounded-md",
          className,
        )}
      >
        <ImageOff className="size-1/3 max-h-10 max-w-10" aria-hidden="true" />
      </div>
    )
  }

  return (
    <img
      src={src}
      alt={name}
      loading="lazy"
      onError={() => setFailed(true)}
      className={cn(
        "bg-base-300 object-cover",
        variant === "card" ? "aspect-[5/7] rounded-xl" : "aspect-[4/3] rounded-md",
        className,
      )}
    />
  )
}
