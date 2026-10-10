import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { ServerLogsPage } from "./server-logs-page"
import { SERVER_LOGS_URL, type ServerLogEntry } from "./server-log-stream"

/** The browser boundary: a controllable stand-in for `EventSource`. */
class FakeEventSource {
  static readonly CONNECTING = 0
  static readonly OPEN = 1
  static readonly CLOSED = 2
  static instances: FakeEventSource[] = []

  readyState = FakeEventSource.CONNECTING
  onerror: ((event: Event) => void) | null = null
  #listeners = new Map<string, Set<(event: MessageEvent) => void>>()

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this)
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    const listeners = this.#listeners.get(type) ?? new Set()
    listeners.add(listener)
    this.#listeners.set(type, listeners)
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void) {
    this.#listeners.get(type)?.delete(listener)
  }

  close() {
    this.readyState = FakeEventSource.CLOSED
  }

  emit(type: string, data: unknown) {
    if (this.readyState === FakeEventSource.CLOSED) return
    this.readyState = FakeEventSource.OPEN
    const event = new MessageEvent(type, { data: JSON.stringify(data) })
    act(() => this.#listeners.get(type)?.forEach((listener) => listener(event)))
  }

  fail(readyState: number) {
    this.readyState = readyState
    act(() => this.onerror?.(new Event("error")))
  }
}

function latest() {
  const source = FakeEventSource.instances.at(-1)
  if (!source) throw new Error("no EventSource opened")
  return source
}

function entry(id: number, overrides: Partial<ServerLogEntry> = {}): ServerLogEntry {
  return {
    id,
    timestamp: "2026-10-10T12:00:00Z",
    level: "info",
    target: "the_gathering::web",
    message: `message ${id}`,
    fields: {},
    request: null,
    ...overrides,
  }
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

async function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({
    routeTree: createRootRoute({ component: ServerLogsPage }),
    history: createMemoryHistory(),
  })
  const view = render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )
  await screen.findByRole("heading", { name: "Server logs" })
  return view
}

beforeEach(() => {
  FakeEventSource.instances = []
  vi.stubGlobal("EventSource", FakeEventSource)
  vi.stubGlobal(
    "fetch",
    vi.fn(() => Promise.resolve(json({ data: { username: "admin", has_password: true } }))),
  )
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

it("connects with the session cookie and shows entries newest first", async () => {
  await renderPage()
  expect(latest().url).toBe(SERVER_LOGS_URL)
  expect(screen.getByRole("status").textContent).toBe("Connecting…")

  latest().emit("ready", { heartbeat_seconds: 5 })
  expect(screen.getByRole("status").textContent).toBe("Live")
  expect(screen.getByText("Waiting for log messages…")).toBeTruthy()

  latest().emit("log", entry(1, { level: "warning", message: "slow query" }))
  latest().emit(
    "log",
    entry(2, {
      level: "error",
      message: "request failed",
      fields: { status: "500" },
      request: { method: "POST", path: "/api/games", request_id: "abcdefghijklmnopqrstu" },
    }),
  )

  const rows = within(screen.getByRole("list", { name: "Server log messages" })).getAllByRole(
    "listitem",
  )
  expect(rows).toHaveLength(2)
  expect(rows[0]?.textContent).toContain("request failed")
  expect(rows[0]?.textContent).toContain("error")
  expect(rows[0]?.textContent).toContain("POST /api/games · abcdefghijklmnopqrstu")
  expect(rows[0]?.textContent).toContain("status=500")
  expect(rows[0]?.querySelector("time")?.getAttribute("dateTime")).toBe("2026-10-10T12:00:00Z")
  expect(rows[1]?.textContent).toContain("slow query")
  expect(rows[1]?.textContent).toContain("warning")
})

it("keeps only the newest 200 items and reports dropped messages", async () => {
  await renderPage()
  latest().emit("ready", {})
  for (let id = 1; id <= 205; id++) latest().emit("log", entry(id))
  latest().emit("gap", { dropped: 3 })

  const list = screen.getByRole("list", { name: "Server log messages" })
  const rows = within(list).getAllByRole("listitem")
  expect(rows).toHaveLength(200)
  expect(rows[0]?.textContent).toContain("3 messages were dropped")
  expect(rows[1]?.textContent).toContain("message 205")
  expect(within(list).queryByText("message 6")).toBeNull()
  expect(within(list).getByText("message 7")).toBeTruthy()
})

it("clears only the local buffer and keeps following the stream", async () => {
  await renderPage()
  latest().emit("ready", {})
  latest().emit("log", entry(1))

  fireEvent.click(screen.getByRole("button", { name: "Clear" }))
  expect(screen.queryByRole("list", { name: "Server log messages" })).toBeNull()
  expect(FakeEventSource.instances).toHaveLength(1)
  expect(latest().readyState).toBe(FakeEventSource.OPEN)
  expect(vi.mocked(fetch)).not.toHaveBeenCalledWith(SERVER_LOGS_URL, expect.anything())

  latest().emit("log", entry(2))
  expect(screen.getByText("message 2")).toBeTruthy()
  expect(screen.queryByText("message 1")).toBeNull()
})

it("closes the stream when the page unmounts", async () => {
  const view = await renderPage()
  const source = latest()
  view.unmount()
  expect(source.readyState).toBe(FakeEventSource.CLOSED)
})

it("shows a dropped connection while the browser reconnects", async () => {
  await renderPage()
  latest().emit("ready", {})
  latest().fail(FakeEventSource.CONNECTING)
  expect(screen.getByRole("status").textContent).toBe("Disconnected — reconnecting…")

  latest().emit("ready", {})
  expect(screen.getByRole("status").textContent).toBe("Live")
  expect(FakeEventSource.instances).toHaveLength(1)
})

it("stops on an unauthorized event and reconnects after confirming the password", async () => {
  const fetch = vi.fn(async (url: string) =>
    url === "/api/session/sudo"
      ? json({ data: { ok: true } })
      : json({ data: { username: "admin", has_password: true } }),
  )
  vi.stubGlobal("fetch", fetch)
  await renderPage()
  latest().emit("ready", {})
  latest().emit("log", entry(1))

  latest().emit("unauthorized", { reason: "sudo_required" })
  expect(latest().readyState).toBe(FakeEventSource.CLOSED)
  expect(FakeEventSource.instances).toHaveLength(1)
  expect(screen.getByRole("status").textContent).toBe("Disconnected")
  // Entries already received stay; the closed stream delivers nothing more.
  latest().emit("log", entry(2))
  expect(screen.queryByText("message 2")).toBeNull()

  fireEvent.change(await screen.findByLabelText("Confirm your password"), {
    target: { value: "long-enough-password" },
  })
  fireEvent.click(screen.getByRole("button", { name: "Confirm password" }))

  await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2))
  expect(screen.getByRole("status").textContent).toBe("Connecting…")
  latest().emit("ready", {})
  expect(screen.getByRole("status").textContent).toBe("Live")
  expect(screen.getByText("message 1")).toBeTruthy()
})

it("asks to sign in again once the session ends, without reconnecting", async () => {
  await renderPage()
  latest().emit("ready", {})
  latest().emit("unauthorized", { reason: "signed_out" })

  expect(screen.getByRole("alert").textContent).toContain("Your session ended.")
  expect(screen.getByRole("link", { name: "Sign in again" })).toBeTruthy()
  expect(FakeEventSource.instances).toHaveLength(1)
})

it("asks for the password when the server refuses the stream for sudo", async () => {
  const fetch = vi.fn(async (url: string) =>
    url === SERVER_LOGS_URL
      ? json({ errors: { code: "sudo_required", detail: "Reauthentication required" } }, 403)
      : json({ data: { username: "admin", has_password: true } }),
  )
  vi.stubGlobal("fetch", fetch)
  await renderPage()
  latest().fail(FakeEventSource.CLOSED)

  expect(await screen.findByText("Confirm it’s you")).toBeTruthy()
  expect(FakeEventSource.instances).toHaveLength(1)
})

it("offers a reconnect when the stream closes for another reason", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response("bad gateway", { status: 502 })),
  )
  await renderPage()
  latest().emit("ready", {})
  latest().fail(FakeEventSource.CLOSED)

  expect(await screen.findByText("The log stream closed.")).toBeTruthy()
  expect(screen.getByRole("status").textContent).toBe("Disconnected")
  fireEvent.click(screen.getByRole("button", { name: "Reconnect" }))
  expect(FakeEventSource.instances).toHaveLength(2)
  latest().emit("ready", {})
  expect(screen.getByRole("status").textContent).toBe("Live")
})
