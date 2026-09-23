import { useLayoutEffect, useRef, useState } from "react"

export const RAIL_WIDTHS = {
  camera: { min: 176, max: 360, initial: 208 },
  panel: { min: 240, max: 480, initial: 288 },
}
type Rail = keyof typeof RAIL_WIDTHS
interface Preferences {
  hotkeys: boolean
  camera: number
  panel: number
}

export function clampRailWidth(rail: Rail, width: number): number {
  const { min, max, initial } = RAIL_WIDTHS[rail]
  return Number.isFinite(width) ? Math.min(max, Math.max(min, width)) : initial
}

export function useTablePreferences(playerId: number) {
  const key = `the-gathering:table-preferences:${playerId}`
  const [preferences, setPreferences] = useState<Preferences>(() => {
    const defaults = {
      hotkeys: true,
      camera: RAIL_WIDTHS.camera.initial,
      panel: RAIL_WIDTHS.panel.initial,
    }
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "null")
      if (!saved || typeof saved !== "object") return defaults
      return {
        hotkeys: "hotkeys" in saved && typeof saved.hotkeys === "boolean" ? saved.hotkeys : true,
        camera:
          "camera" in saved && typeof saved.camera === "number"
            ? clampRailWidth("camera", saved.camera)
            : defaults.camera,
        panel:
          "panel" in saved && typeof saved.panel === "number"
            ? clampRailWidth("panel", saved.panel)
            : defaults.panel,
      }
    } catch {
      return defaults
    }
  })
  // Save before paint so a reload immediately after a drag cannot lose the visible width.
  useLayoutEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(preferences))
    } catch {
      /* Storage may be disabled. */
    }
  }, [key, preferences])
  return {
    ...preferences,
    setHotkeys: (hotkeys: boolean) => setPreferences((value) => ({ ...value, hotkeys })),
    setWidth: (rail: Rail, width: number) =>
      setPreferences((value) => ({ ...value, [rail]: clampRailWidth(rail, width) })),
    resetWidths: () =>
      setPreferences((value) => ({
        ...value,
        camera: RAIL_WIDTHS.camera.initial,
        panel: RAIL_WIDTHS.panel.initial,
      })),
  }
}

/** Pointer capture keeps the drag alive outside the narrow handle; keyboard arrows resize too. */
export function RailResizeHandle({
  rail,
  width,
  onChange,
}: {
  rail: Rail
  width: number
  onChange: (width: number) => void
}) {
  const drag = useRef<{ x: number; width: number } | null>(null)
  const direction = rail === "camera" ? 1 : -1
  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={rail === "camera" ? "Camera rail width" : "Side panel width"}
      aria-orientation="vertical"
      aria-valuemin={RAIL_WIDTHS[rail].min}
      aria-valuemax={RAIL_WIDTHS[rail].max}
      aria-valuenow={Math.round(width)}
      title="Drag to resize · double-click to reset · arrow keys to adjust"
      className="hidden w-1.5 touch-none cursor-col-resize bg-white/5 hover:bg-primary/60 focus-visible:bg-primary focus-visible:outline-none lg:block"
      onDoubleClick={() => onChange(RAIL_WIDTHS[rail].initial)}
      onPointerDown={(event) => {
        if (event.button !== 0) return
        event.preventDefault()
        event.currentTarget.focus()
        event.currentTarget.setPointerCapture(event.pointerId)
        drag.current = { x: event.clientX, width }
      }}
      onPointerMove={(event) => {
        if (drag.current)
          onChange(
            clampRailWidth(rail, drag.current.width + direction * (event.clientX - drag.current.x)),
          )
      }}
      onPointerUp={(event) => {
        drag.current = null
        event.currentTarget.releasePointerCapture(event.pointerId)
      }}
      onLostPointerCapture={() => {
        drag.current = null
      }}
      onKeyDown={(event) => {
        if (!["ArrowLeft", "ArrowRight", "Home"].includes(event.key)) return
        event.preventDefault()
        onChange(
          event.key === "Home"
            ? RAIL_WIDTHS[rail].initial
            : clampRailWidth(rail, width + (event.key === "ArrowRight" ? 16 : -16) * direction),
        )
      }}
    />
  )
}

export function TableSettings({
  hotkeys,
  onHotkeysChange,
  onResetWidths,
  onHelp,
}: {
  hotkeys: boolean
  onHotkeysChange: (enabled: boolean) => void
  onResetWidths: () => void
  onHelp: () => void
}) {
  return (
    <section className="grid gap-4 p-3 text-xs" aria-label="Table settings">
      <h2 className="font-bold">Table settings</h2>
      <p className="text-base-content/60">Saved for your player in this browser.</p>
      <label className="flex items-center justify-between gap-2">
        Keyboard shortcuts
        <input
          type="checkbox"
          className="toggle toggle-sm"
          checked={hotkeys}
          onChange={(event) => onHotkeysChange(event.target.checked)}
        />
      </label>
      <button type="button" className="btn btn-sm btn-outline" onClick={onHelp}>
        Keyboard shortcut help
      </button>
      <button type="button" className="btn btn-sm btn-outline" onClick={onResetWidths}>
        Reset rail widths
      </button>
      <p className="text-base-content/60">
        On desktop, drag either divider to resize. Double-click a divider to reset it, or focus it
        and use arrow keys (Home resets).
      </p>
    </section>
  )
}
