// Routes are code-split into hashed chunks. After a deploy the old chunks are
// gone, so a tab that loaded the previous bundle 404s the first time it
// navigates to a route it has not visited yet. Vite reports that as a
// cancelable `vite:preloadError` event; we reload once to pick up the new
// shell, and give up if the reload did not help so a real outage does not
// turn into a reload loop.

export const RELOAD_KEY = "the-gathering:stale-bundle-reload"
export const RELOAD_WINDOW_MS = 60_000

export interface StaleBundleEnv {
  now: () => number
  reload: () => void
  storage: Pick<Storage, "getItem" | "setItem" | "removeItem">
}

function lastReloadAt(storage: StaleBundleEnv["storage"]): number | null {
  try {
    const value = storage.getItem(RELOAD_KEY)
    if (value === null) return null
    const at = Number(value)
    return Number.isFinite(at) ? at : null
  } catch {
    return null
  }
}

/**
 * Decides whether a failed chunk load should trigger a reload. Returns true
 * (and records the reload) unless this tab already reloaded for the same
 * reason within the last minute.
 */
export function handleStaleBundle(env: StaleBundleEnv): boolean {
  const now = env.now()
  const previous = lastReloadAt(env.storage)
  if (previous !== null && now - previous < RELOAD_WINDOW_MS) return false

  try {
    env.storage.setItem(RELOAD_KEY, String(now))
  } catch {
    // Without storage we cannot guard against loops, so do not reload.
    return false
  }
  env.reload()
  return true
}

export function installStaleBundleReload(): void {
  window.addEventListener("vite:preloadError", (event) => {
    const reloaded = handleStaleBundle({
      now: Date.now,
      reload: () => window.location.reload(),
      storage: window.sessionStorage,
    })
    // Swallow the error only when we are actually reloading; otherwise let it
    // surface so the router's error boundary can show something.
    if (reloaded) event.preventDefault()
  })
}
