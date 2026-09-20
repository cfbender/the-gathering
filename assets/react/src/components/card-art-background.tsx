import { cn } from "@/lib/cn"

export function CardArtBackground({
  imageUrl,
  interactive = false,
}: {
  imageUrl?: string | null
  interactive?: boolean
}) {
  if (!imageUrl) return null

  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 overflow-hidden">
      <img
        src={imageUrl}
        alt=""
        loading="lazy"
        className={cn(
          "absolute inset-0 h-full w-full object-cover opacity-75 transition duration-300",
          interactive && "group-hover:opacity-45",
        )}
      />
      <div className="from-base-100/98 via-base-100/25 absolute inset-0 bg-gradient-to-br to-transparent" />
    </div>
  )
}
