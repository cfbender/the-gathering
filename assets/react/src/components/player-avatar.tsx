import { cn } from "@/lib/cn"

interface PlayerAvatarProps {
  name: string
  avatarUrl?: string | null
  size?: "md" | "lg"
  className?: string
}

const sizes = {
  md: { ring: "w-12", text: "text-lg" },
  lg: { ring: "w-20", text: "text-3xl" },
}

/** Discord avatar when the player is linked to a user, otherwise their initials. */
export function PlayerAvatar({ name, avatarUrl, size = "md", className }: PlayerAvatarProps) {
  const { ring, text } = sizes[size]
  if (avatarUrl) {
    return (
      <div className={cn("avatar", className)}>
        <div className={cn("rounded-full", ring)}>
          <img src={avatarUrl} alt="" referrerPolicy="no-referrer" />
        </div>
      </div>
    )
  }
  return (
    <div className={cn("avatar avatar-placeholder", className)}>
      <div className={cn("bg-primary text-primary-content rounded-full", ring)}>
        <span className={cn("font-bold", text)}>{initials(name)}</span>
      </div>
    </div>
  )
}

export function initials(name: string) {
  const [first = "?", second] = name.trim().split(/\s+/).filter(Boolean)
  const letters = second ? `${first.charAt(0)}${second.charAt(0)}` : first.slice(0, 2)
  return letters.toUpperCase()
}
