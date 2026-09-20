import { Monitor, Moon, Sun } from "lucide-react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import { useTheme } from "@/lib/theme"
import type { ThemePreference } from "@/lib/theme"

const options: { value: ThemePreference; label: string; Icon: typeof Sun }[] = [
  { value: "system", label: "Match system theme", Icon: Monitor },
  { value: "light", label: "Light theme", Icon: Sun },
  { value: "dark", label: "Dark theme", Icon: Moon },
]

const thumbOffset: Record<ThemePreference, string> = {
  system: "translate-x-0",
  light: "translate-x-full",
  dark: "translate-x-[200%]",
}

export function ThemeToggle({ onSelect }: { onSelect?: () => void }) {
  const { preference, setPreference } = useTheme()

  return (
    <ToggleGroup
      type="single"
      value={preference}
      aria-label="Theme"
      onValueChange={(value) => {
        // Radix allows deselecting the pressed item; a theme must always be chosen.
        if (!value) return
        setPreference(value as ThemePreference)
        onSelect?.()
      }}
      className="border-base-300 bg-base-200 relative grid h-11 w-36 grid-cols-3 rounded-full border p-1 shadow-sm"
    >
      <span
        aria-hidden="true"
        className={cn(
          "bg-primary/15 ring-primary/25 absolute inset-y-1 left-1 w-[calc((100%-0.5rem)/3)] rounded-full ring-1 transition-transform",
          thumbOffset[preference],
        )}
      />
      {options.map(({ value, label, Icon }) => (
        <ToggleGroupItem
          key={value}
          value={value}
          title={label}
          aria-label={label}
          className={cn(
            "hover:text-primary focus-visible:ring-primary/35 relative z-10 flex h-full w-full items-center justify-center rounded-full transition-colors focus-visible:ring-2 focus-visible:outline-none",
            preference === value ? "text-primary" : "text-base-content/70",
          )}
        >
          <Icon className="size-4" aria-hidden="true" />
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
