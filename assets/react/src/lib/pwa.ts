import { useSyncExternalStore } from "react"

/** Chrome's install prompt event, which TypeScript's DOM types do not include. */
interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>
}

let installPrompt: BeforeInstallPromptEvent | null = null
const listeners = new Set<() => void>()

function setInstallPrompt(prompt: BeforeInstallPromptEvent | null) {
  installPrompt = prompt
  listeners.forEach((listener) => listener())
}

/**
 * Registers the service worker (`/sw.js`, which gives page loads an offline fallback) and
 * holds Chrome's install prompt so the app can offer its own Install button. Runs before React
 * renders: the prompt event can fire before any component mounts.
 */
export function installPwa() {
  window.addEventListener("beforeinstallprompt", (event) => {
    event.preventDefault()
    setInstallPrompt(event as BeforeInstallPromptEvent)
  })
  window.addEventListener("appinstalled", () => setInstallPrompt(null))
  if ("serviceWorker" in navigator && window.isSecureContext) {
    window.addEventListener("load", () => {
      navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => {
        // Without the worker the app still works online; it just cannot be installed.
      })
    })
  }
}

/** Shows the browser's install dialog; the prompt can be used once either way. */
export async function promptInstall() {
  const prompt = installPrompt
  if (!prompt) return
  setInstallPrompt(null)
  await prompt.prompt()
  await prompt.userChoice.catch(() => undefined)
}

/** Whether the browser offers to install the app right now (never once it is installed). */
export function useCanInstall() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    () => installPrompt !== null,
  )
}
