import { useCallback, useEffect, useRef, useState } from "react"

/**
 * Live server logs over Server-Sent Events (`GET /api/admin/server-logs`).
 *
 * The same-origin `EventSource` sends the session cookie; the route needs an administrator
 * with a recent password confirmation. Events (JSON `data`):
 * - `ready`: the stream is live.
 * - `log`: one {@link ServerLogEntry}.
 * - `gap` `{ dropped }`: entries the server dropped because this stream fell behind.
 * - `unauthorized` `{ reason }`: terminal; the session lost access, so stop and ask the user
 *   to reauthenticate instead of reconnecting.
 *
 * Nothing is persisted: the browser keeps the newest {@link MAX_LOG_ITEMS} items in memory.
 */
export const SERVER_LOGS_URL = "/api/admin/server-logs"
export const MAX_LOG_ITEMS = 200

export type ServerLogLevel = "debug" | "info" | "warning" | "error"

export interface ServerLogEntry {
  id: number
  timestamp: string
  level: ServerLogLevel
  target: string
  message: string
  fields: Record<string, string>
  request: { method: string; path: string; request_id: string } | null
}

export type ServerLogItem =
  | { kind: "entry"; key: string; entry: ServerLogEntry }
  | { kind: "gap"; key: string; dropped: number }

export type UnauthorizedReason = "signed_out" | "forbidden" | "sudo_required"

export type ServerLogConnection =
  | { status: "connecting" }
  | { status: "live" }
  | { status: "disconnected"; retrying: boolean }
  | { status: "unauthorized"; reason: UnauthorizedReason }

function parse<T>(data: unknown): T | null {
  if (typeof data !== "string") return null
  try {
    return JSON.parse(data) as T
  } catch {
    return null
  }
}

function reasonFrom(value: unknown): UnauthorizedReason {
  return value === "forbidden" || value === "sudo_required" ? value : "signed_out"
}

/**
 * Asks the route why the browser refused the stream: `EventSource` hides the status of a
 * failed connection. The response body of a stream that did open is abandoned unread.
 */
async function diagnoseRefusal(): Promise<ServerLogConnection> {
  const controller = new AbortController()
  try {
    const response = await fetch(SERVER_LOGS_URL, {
      headers: { accept: "application/json" },
      credentials: "same-origin",
      signal: controller.signal,
    })
    if (response.status === 401) return { status: "unauthorized", reason: "signed_out" }
    if (response.status === 403) {
      const body = parse<{ errors?: { code?: string } }>(await response.text())
      return {
        status: "unauthorized",
        reason: body?.errors?.code === "sudo_required" ? "sudo_required" : "forbidden",
      }
    }
    return { status: "disconnected", retrying: false }
  } catch {
    return { status: "disconnected", retrying: false }
  } finally {
    controller.abort()
  }
}

/** Follows the live server log stream while mounted. */
export function useServerLogStream() {
  const [items, setItems] = useState<ServerLogItem[]>([])
  const [connection, setConnection] = useState<ServerLogConnection>({ status: "connecting" })
  const [attempt, setAttempt] = useState(0)
  const gapCount = useRef(0)

  useEffect(() => {
    let active = true
    const source = new EventSource(SERVER_LOGS_URL)
    const prepend = (item: ServerLogItem) =>
      setItems((current) => [item, ...current].slice(0, MAX_LOG_ITEMS))

    source.addEventListener("ready", () => setConnection({ status: "live" }))
    source.addEventListener("log", (event) => {
      const entry = parse<ServerLogEntry>(event.data)
      if (entry) prepend({ kind: "entry", key: `log-${entry.id}`, entry })
    })
    source.addEventListener("gap", (event) => {
      const dropped = parse<{ dropped?: number }>(event.data)?.dropped ?? 0
      gapCount.current += 1
      prepend({ kind: "gap", key: `gap-${gapCount.current}`, dropped })
    })
    source.addEventListener("unauthorized", (event) => {
      source.close()
      const reason = reasonFrom(parse<{ reason?: string }>(event.data)?.reason)
      setConnection({ status: "unauthorized", reason })
    })
    source.onerror = () => {
      if (source.readyState !== EventSource.CLOSED) {
        // The browser reconnects by itself after a dropped connection.
        setConnection({ status: "disconnected", retrying: true })
        return
      }
      source.close()
      void diagnoseRefusal().then((next) => {
        if (active) setConnection(next)
      })
    }

    return () => {
      active = false
      source.close()
    }
  }, [attempt])

  const reconnect = useCallback(() => {
    setConnection({ status: "connecting" })
    setAttempt((current) => current + 1)
  }, [])
  const clear = useCallback(() => setItems([]), [])

  return { items, connection, reconnect, clear }
}
