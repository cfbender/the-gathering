import { useLayoutEffect, useRef, useState } from "react"
import type { FlipAxis } from "./board"
import { isPublisherQuality, type PublisherQuality } from "./media-policy"

export const RAIL_WIDTHS = {
  camera: { min: 176, max: 360, initial: 240 },
  panel: { min: 240, max: 480, initial: 288 },
}
type Rail = keyof typeof RAIL_WIDTHS
interface Preferences {
  hotkeys: boolean
  camera: number
  panel: number
  followTurn: boolean
  panelLeft: boolean
  deviceId: string
  cameraEnabled: boolean
  quality: PublisherQuality
  stats: boolean
  turnSound: boolean
  /** Remote players whose video this viewer flips vertically (saved before horizontal flips existed). */
  flippedPlayerIds: number[]
  horizontallyFlippedPlayerIds: number[]
}

const FLIP_KEYS = {
  vertical: "flippedPlayerIds",
  horizontal: "horizontallyFlippedPlayerIds",
} as const satisfies Record<FlipAxis, keyof Preferences>

function savedPlayerIds(saved: object, key: string): number[] {
  const ids: unknown = key in saved ? (saved as Record<string, unknown>)[key] : undefined
  return Array.isArray(ids)
    ? ids.filter((id): id is number => typeof id === "number" && Number.isSafeInteger(id) && id > 0)
    : []
}

export function clampRailWidth(rail: Rail, width: number): number {
  const { min, max, initial } = RAIL_WIDTHS[rail]
  return Number.isFinite(width) ? Math.min(max, Math.max(min, width)) : initial
}

export function useTablePreferences(playerId: number) {
  const key = `the-gathering:table-preferences:${playerId}`
  const [preferences, setPreferences] = useState<Preferences>(() => {
    const defaults: Preferences = {
      hotkeys: true,
      camera: RAIL_WIDTHS.camera.initial,
      panel: RAIL_WIDTHS.panel.initial,
      followTurn: false,
      panelLeft: false,
      deviceId: "",
      cameraEnabled: true,
      quality: "auto",
      stats: false,
      turnSound: true,
      flippedPlayerIds: [],
      horizontallyFlippedPlayerIds: [],
    }
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "null")
      if (!saved || typeof saved !== "object") return defaults
      return {
        ...defaults,
        followTurn: "followTurn" in saved && saved.followTurn === true,
        panelLeft: "panelLeft" in saved && saved.panelLeft === true,
        deviceId: "deviceId" in saved && typeof saved.deviceId === "string" ? saved.deviceId : "",
        cameraEnabled: !("cameraEnabled" in saved && saved.cameraEnabled === false),
        quality: "quality" in saved && isPublisherQuality(saved.quality) ? saved.quality : "auto",
        stats: "stats" in saved && saved.stats === true,
        turnSound: !("turnSound" in saved && saved.turnSound === false),
        flippedPlayerIds: savedPlayerIds(saved, FLIP_KEYS.vertical),
        horizontallyFlippedPlayerIds: savedPlayerIds(saved, FLIP_KEYS.horizontal),
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
    toggleVideoFlip: (remotePlayerId: number, axis: FlipAxis) =>
      setPreferences((value) => {
        const ids = value[FLIP_KEYS[axis]]
        return {
          ...value,
          [FLIP_KEYS[axis]]: ids.includes(remotePlayerId)
            ? ids.filter((id) => id !== remotePlayerId)
            : [...ids, remotePlayerId],
        }
      }),
    update: (changes: Partial<Preferences>) =>
      setPreferences((value) => ({ ...value, ...changes })),
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
  reversed = false,
  onChange,
}: {
  rail: Rail
  width: number
  reversed?: boolean
  onChange: (width: number) => void
}) {
  const drag = useRef<{ x: number; width: number } | null>(null)
  const direction = (rail === "camera" ? 1 : -1) * (reversed ? -1 : 1)
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
      style={reversed ? { order: rail === "camera" ? 4 : 2 } : undefined}
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
