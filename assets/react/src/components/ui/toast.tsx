import { X } from "lucide-react"
import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react"
import type { ReactNode } from "react"
import { cn } from "@/lib/cn"
import { overlayLayers } from "./overlay-layers"

export type ToastTone = "error" | "success" | "info"

export interface ToastOptions {
  message: string
  tone?: ToastTone
  /** Milliseconds before the toast dismisses itself; `null` keeps it until closed. */
  duration?: number | null
}

interface ToastEntry extends Required<Omit<ToastOptions, "duration">> {
  id: number
}

interface ToastContextValue {
  /** Shows a toast and returns its id so callers can dismiss it early. */
  toast: (options: ToastOptions) => number
  dismiss: (id: number) => void
}

const ToastContext = createContext<ToastContextValue | null>(null)

const DEFAULT_DURATION = 6_000

/** Bottom-right stack of daisyUI alerts; mount once above the router so error
 * boundaries and mutations anywhere can announce failures. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastEntry[]>([])
  const nextId = useRef(0)

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id))
  }, [])

  const toast = useCallback(
    ({ message, tone = "error", duration = DEFAULT_DURATION }: ToastOptions) => {
      const id = nextId.current++
      setToasts((current) => [...current, { id, message, tone }])
      if (duration !== null) window.setTimeout(() => dismiss(id), duration)
      return id
    },
    [dismiss],
  )

  const value = useMemo(() => ({ toast, dismiss }), [toast, dismiss])

  return (
    <ToastContext.Provider value={value}>
      {children}
      {toasts.length > 0 && (
        <div
          className="toast toast-end toast-bottom max-w-sm"
          style={{ zIndex: overlayLayers.floating }}
        >
          {toasts.map((entry) => (
            <div
              key={entry.id}
              role={entry.tone === "error" ? "alert" : "status"}
              className={cn(
                "alert shadow-lg",
                entry.tone === "error" && "alert-error",
                entry.tone === "success" && "alert-success",
                entry.tone === "info" && "alert-info",
              )}
            >
              <span className="whitespace-normal text-sm">{entry.message}</span>
              <button
                type="button"
                className="btn btn-ghost btn-xs btn-square"
                aria-label="Dismiss"
                onClick={() => dismiss(entry.id)}
              >
                <X className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}
    </ToastContext.Provider>
  )
}

export function useToast(): ToastContextValue {
  const context = useContext(ToastContext)
  if (!context) throw new Error("useToast must be used within ToastProvider")
  return context
}
