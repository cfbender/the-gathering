import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { SoftwareUpdate, type SoftwareUpdateStatus } from "./software-update"

const base: SoftwareUpdateStatus = {
  version: "v0.1.0",
  channel: "release",
  method: "systemd",
  pending: false,
  requested_at: null,
  latest: {
    version: "v0.2.0",
    url: "https://github.com/cfbender/the-gathering/releases/tag/v0.2.0",
  },
  update_available: true,
  check_error: null,
}

function renderSection() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <SoftwareUpdate />
    </QueryClientProvider>,
  )
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

it("explains when no updater is configured and keeps the button disabled", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() =>
      Promise.resolve(
        Response.json({
          data: { ...base, version: null, channel: null, method: null, latest: null },
        }),
      ),
    ),
  )
  renderSection()

  await screen.findByText("Running a development build.")
  expect(screen.getByText(/Updates cannot be started from here/)).toBeTruthy()
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Update now" }).disabled).toBe(true)
})

it("shows the newer release, only updates after confirmation, and reports the new version", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  let requests = 0
  let statusCalls = 0
  vi.stubGlobal(
    "fetch",
    vi.fn((_url: string, init?: RequestInit) => {
      if (init?.method === "POST") {
        requests++
        return Promise.resolve(
          Response.json(
            { data: { ...base, pending: true, requested_at: "2026-10-02T18:00:00Z" } },
            { status: 202 },
          ),
        )
      }
      statusCalls++
      // Before the request: idle. First poll after it: the updater is still running. Then the
      // restarted server answers with the new version.
      const data =
        requests === 0
          ? base
          : statusCalls <= 2
            ? { ...base, pending: true, requested_at: "2026-10-02T18:00:00Z" }
            : { ...base, version: "v0.2.0", update_available: false }
      return Promise.resolve(Response.json({ data }))
    }),
  )
  renderSection()

  await screen.findByText("is available.", { exact: false })
  expect(screen.getByRole("link", { name: "What changed" }).getAttribute("href")).toBe(
    base.latest?.url,
  )
  const button = await screen.findByRole<HTMLButtonElement>("button", { name: "Update now" })
  await waitFor(() => expect(button.disabled).toBe(false))

  fireEvent.click(button)
  const dialog = await screen.findByRole("dialog", { name: "Update the server now?" })
  expect(within(dialog).getByText(/installs v0\.2\.0 and restarts/)).toBeTruthy()
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }))
  expect(requests).toBe(0)

  fireEvent.click(screen.getByRole("button", { name: "Update now" }))
  fireEvent.click(
    within(await screen.findByRole("dialog")).getByRole("button", { name: "Update now" }),
  )
  await screen.findByText(/Update requested at/)
  expect(requests).toBe(1)
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Updating…" }).disabled).toBe(true)

  await vi.advanceTimersByTimeAsync(5_000)
  await vi.advanceTimersByTimeAsync(5_000)
  await screen.findByText("Updated to v0.2.0.")
  expect(screen.getByText(/Running/).textContent).toContain("v0.2.0")
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Update now" }).disabled).toBe(false)
})

it("surfaces a failed update check without hiding the button", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() =>
      Promise.resolve(
        Response.json({
          data: {
            ...base,
            method: "watchtower",
            latest: null,
            update_available: null,
            check_error: "Could not reach GitHub.",
          },
        }),
      ),
    ),
  )
  renderSection()

  await screen.findByText("Could not check for updates: Could not reach GitHub.")
  expect(screen.getByText(/asks Watchtower/)).toBeTruthy()
  await waitFor(() =>
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Update now" }).disabled).toBe(
      false,
    ),
  )
})
