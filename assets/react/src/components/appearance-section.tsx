import { Check, Droplets, Square } from "lucide-react"
import { cn } from "@/lib/cn"
import { PALETTES, useTheme } from "@/lib/theme"
import type { ThemeStyle } from "@/lib/theme"

const styleOptions: {
  value: ThemeStyle
  label: string
  description: string
  Icon: typeof Square
}[] = [
  {
    value: "classic",
    label: "Classic",
    description: "Solid, tactile surfaces with crisp borders and compact corners.",
    Icon: Square,
  },
  {
    value: "glass",
    label: "Liquid glass",
    description:
      "Translucent, blurred panels over an ambient backdrop, in the spirit of modern macOS.",
    Icon: Droplets,
  },
]

/** Palette and interface style pickers; the light/dark toggle lives in the header and applies to both. */
export function AppearanceSection() {
  const { themeStyle, setThemeStyle, palette, setPalette, resolved } = useTheme()

  return (
    <section className="card border-base-300 bg-base-200 border" aria-labelledby="appearance">
      <div className="card-body gap-4">
        <div>
          <h2 id="appearance" className="card-title text-lg">
            <Droplets className="size-5" /> Appearance
          </h2>
          <p className="text-base-content/60 mt-1 text-sm">
            Choose a color palette and how surfaces render; both follow your account to every
            device. Light and dark modes stay on the toggle in the navigation, and every palette has
            both.
          </p>
        </div>
        <h3 className="text-sm font-bold">Palette</h3>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {PALETTES.map(({ id, label }) => {
            const selected = palette === id

            return (
              <button
                key={id}
                type="button"
                aria-pressed={selected}
                onClick={() => setPalette(id)}
                className={cn(
                  "rounded-box focus-visible:ring-primary/35 flex flex-col gap-2 border p-2 text-left transition-colors focus-visible:ring-2 focus-visible:outline-none",
                  selected
                    ? "border-primary/50 bg-primary/10"
                    : "border-base-300 bg-base-100/40 hover:border-primary/40",
                )}
              >
                {/* Scoping data-palette/data-theme to the swatch previews its tokens in the current mode. */}
                <span
                  aria-hidden="true"
                  data-palette={id}
                  data-theme={resolved}
                  className="bg-base-100 text-base-content border-base-300 flex h-12 items-center justify-between rounded-[calc(var(--radius-box)*0.6)] border px-3"
                >
                  <span className="text-sm font-bold">Aa</span>
                  <span className="flex -space-x-1">
                    {["bg-primary", "bg-secondary", "bg-accent"].map((color) => (
                      <span
                        key={color}
                        className={cn("ring-base-100 size-4 rounded-full ring-2", color)}
                      />
                    ))}
                  </span>
                </span>
                <span className="flex items-center gap-1.5 px-1 text-sm font-bold">
                  {label}
                  {selected ? <Check className="text-primary size-4" aria-hidden="true" /> : null}
                </span>
              </button>
            )
          })}
        </div>
        <h3 className="text-sm font-bold">Surface style</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          {styleOptions.map(({ value, label, description, Icon }) => {
            const selected = themeStyle === value

            return (
              <button
                key={value}
                type="button"
                aria-pressed={selected}
                onClick={() => setThemeStyle(value)}
                className={cn(
                  "rounded-box focus-visible:ring-primary/35 flex items-start gap-3 border p-4 text-left transition-colors focus-visible:ring-2 focus-visible:outline-none",
                  selected
                    ? "border-primary/50 bg-primary/10"
                    : "border-base-300 bg-base-100/40 hover:border-primary/40",
                )}
              >
                <Icon
                  className={cn(
                    "mt-0.5 size-5 shrink-0",
                    selected ? "text-primary" : "text-base-content/70",
                  )}
                  aria-hidden="true"
                />
                <span className="min-w-0">
                  <span className="flex items-center gap-2 font-bold">
                    {label}
                    {selected ? <Check className="text-primary size-4" aria-hidden="true" /> : null}
                  </span>
                  <span className="text-base-content/60 mt-1 block text-sm">{description}</span>
                </span>
              </button>
            )
          })}
        </div>
      </div>
    </section>
  )
}
