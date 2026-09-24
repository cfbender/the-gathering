import { Link } from "@tanstack/react-router"
import type { ErrorComponentProps } from "@tanstack/react-router"
import { useEffect } from "react"
import { useToast } from "@/components/ui/toast"
import { ApiError } from "@/lib/api"

/** One line a player can act on; API responses keep their detail, render crashes get a generic note. */
export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.detail) return error.detail
    if (error.status === 422) return "The server rejected the change. Check the form and try again."
    return `Request failed (${error.status}).`
  }
  if (error instanceof Error && error.message) return error.message
  return "Something went wrong."
}

/**
 * Route-level error boundary: an unhandled render or loader error toasts once
 * and swaps the failing route for a recoverable panel instead of the router's
 * raw "Something went wrong!" dump.
 */
export function RouteError({ error, reset }: ErrorComponentProps) {
  const { toast, dismiss } = useToast()
  const message = errorMessage(error)

  // Dismissing on cleanup keeps StrictMode's double mount to one toast and
  // clears it when a retry succeeds and the panel goes away.
  useEffect(() => {
    const id = toast({ message, tone: "error" })
    return () => dismiss(id)
  }, [message, toast, dismiss])

  return (
    <section
      role="alert"
      className="card border-base-300 bg-base-200 mx-auto max-w-md border p-10 text-center"
    >
      <p className="text-error font-mono text-sm">Error</p>
      <h1 className="mt-2 text-2xl font-black tracking-normal">Something went wrong</h1>
      <p className="text-base-content/70 mt-2 break-words">{message}</p>
      <div className="mt-6 flex flex-wrap justify-center gap-2">
        <button type="button" className="btn btn-primary" onClick={reset}>
          Try again
        </button>
        <Link to="/" className="btn btn-outline">
          Back to the games
        </Link>
      </div>
    </section>
  )
}
