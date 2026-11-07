import { useCallback, useSyncExternalStore } from "react"
import { DEFAULT_SETTINGS, type TabletopGame, type TabletopSettings } from "./tabletop"

/** The game in progress and the last setup live in this browser, so a reload, a locked phone,
 * or reopening the installed app picks up where the table left off. */
const GAME_KEY = "the-gathering:tabletop-game"
const SETTINGS_KEY = "the-gathering:tabletop-settings"

const listeners = new Set<() => void>()
const cache = new Map<string, { raw: string | null; value: unknown }>()

function read<T>(key: string, fallback: T): T {
  let raw: string | null = null
  try {
    raw = localStorage.getItem(key)
  } catch {
    // Storage can be unavailable (private windows); the table still works for this page.
  }
  const cached = cache.get(key)
  if (cached && cached.raw === raw) return cached.value as T
  let value: T = fallback
  try {
    value = raw === null ? fallback : (JSON.parse(raw) as T)
  } catch {
    value = fallback
  }
  cache.set(key, { raw, value })
  return value
}

function write(key: string, value: unknown) {
  try {
    if (value === null) localStorage.removeItem(key)
    else localStorage.setItem(key, JSON.stringify(value))
  } catch {
    cache.set(key, { raw: null, value })
  }
  listeners.forEach((listener) => listener())
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  const onStorage = () => listener()
  window.addEventListener("storage", onStorage)
  return () => {
    listeners.delete(listener)
    window.removeEventListener("storage", onStorage)
  }
}

export function useTabletopGame() {
  const game = useSyncExternalStore(subscribe, () => read<TabletopGame | null>(GAME_KEY, null))
  const setGame = useCallback(
    (next: TabletopGame | null | ((current: TabletopGame) => TabletopGame)) => {
      if (typeof next === "function") {
        const current = read<TabletopGame | null>(GAME_KEY, null)
        if (current) write(GAME_KEY, next(current))
        return
      }
      write(GAME_KEY, next)
    },
    [],
  )
  return [game, setGame] as const
}

export function useTabletopSettings() {
  const settings = useSyncExternalStore(subscribe, () =>
    read<TabletopSettings>(SETTINGS_KEY, DEFAULT_SETTINGS),
  )
  const setSettings = useCallback((next: TabletopSettings) => write(SETTINGS_KEY, next), [])
  return [settings, setSettings] as const
}
