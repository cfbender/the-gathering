import { Monitor, Moon, Sun } from "lucide-react"
import { cn } from "@/lib/cn"
import { useTheme } from "@/lib/theme"
import type { ThemePreference } from "@/lib/theme"

const options: { value: ThemePreference; label: string; Icon: typeof Sun }[] = [
  { value: "light", label: "Light theme", Icon: Sun },
  { value: "system", label: "Match system theme", Icon: Monitor },
  { value: "dark", label: "Dark theme", Icon: Moon },
]

export function ThemeToggle() {
  const { preference, setPreference } = useTheme()

  return (
    <div
      role="radiogroup"
      aria-label="Theme"
      className="bg-base-200 border-base-300 flex items-center gap-0.5 rounded-full border p-0.5"
    >
      {options.map(({ value, label, Icon }) => {
        const active = preference === value
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={active}
            aria-label={label}
            title={label}
            onClick={() => setPreference(value)}
            className={cn(
              "grid size-7 place-items-center rounded-full transition-colors",
              active
                ? "bg-base-100 text-base-content shadow-sm"
                : "text-base-content/60 hover:text-base-content",
            )}
          >
            <Icon className="size-4" aria-hidden="true" />
          </button>
        )
      })}
    </div>
  )
}
