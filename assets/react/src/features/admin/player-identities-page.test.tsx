import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { PlayerIdentitiesPage } from "./player-identities-page"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({
    routeTree: createRootRoute({ component: PlayerIdentitiesPage }),
    history: createMemoryHistory(),
  })
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

const player = {
  id: 42,
  name: "Imported player",
  discord_id: "123456789012345678",
  archived_at: "2026-09-20T12:00:00Z",
  user: { id: 9, username: "old_account" },
}

it("confirms unlinking, preserves the player, and refreshes the identity", async () => {
  let unlinked = false
  const fetch = vi.fn(async (url: string, init: RequestInit) => {
    if (init.method === "DELETE") {
      unlinked = true
      return new Response(null, { status: 204 })
    }
    if (url === "/api/session") return json({ data: { has_password: true } })
    return json({
      data: [unlinked ? { ...player, discord_id: null, user: null } : player],
      meta: { total: 1, page: 1, total_pages: 1 },
    })
  })
  vi.stubGlobal("fetch", fetch)
  renderPage()

  expect(await screen.findByText(player.discord_id)).toBeTruthy()
  expect(screen.getByText("@old_account")).toBeTruthy()
  expect(screen.getByText("Archived")).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Unlink identity for Imported player" }))
  const dialog = screen.getByRole("alertdialog")
  expect(within(dialog).getByText(/account will be detached/)).toBeTruthy()
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }))
  expect(unlinked).toBe(false)

  fireEvent.click(screen.getByRole("button", { name: "Unlink identity for Imported player" }))
  fireEvent.click(
    within(screen.getByRole("alertdialog")).getByRole("button", { name: "Unlink identity" }),
  )
  expect(await screen.findByText("Not linked")).toBeTruthy()
  expect(screen.getByText("No account")).toBeTruthy()
  expect(screen.getByRole("link", { name: "Imported player" })).toBeTruthy()
  expect(
    screen
      .getByRole("button", { name: "Unlink identity for Imported player" })
      .hasAttribute("disabled"),
  ).toBe(true)
  expect(fetch).toHaveBeenCalledWith(
    "/api/admin/players/42/identity",
    expect.objectContaining({ method: "DELETE" }),
  )
})

it("resets pagination on search and handles empty results", async () => {
  const fetch = vi.fn(async (url: string) => {
    if (url === "/api/session") return json({ data: { has_password: true } })
    const params = new URL(url, "https://example.test").searchParams
    return json({
      data: params.get("search") ? [] : [player],
      meta: { total: 51, page: Number(params.get("page")), total_pages: 2 },
    })
  })
  vi.stubGlobal("fetch", fetch)
  renderPage()
  fireEvent.click(await screen.findByRole("button", { name: "Next" }))
  await screen.findByText(/Page 2 of 2/)
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "missing" } })
  expect(await screen.findByText("No players found.")).toBeTruthy()
  await waitFor(() =>
    expect(fetch).toHaveBeenCalledWith(
      "/api/admin/players?search=missing&page=1",
      expect.anything(),
    ),
  )
})

it("shows reauthentication when identity access requires sudo", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      url === "/api/session"
        ? json({ data: { has_password: true } })
        : json({ errors: { code: "sudo_required", detail: "Reauthentication required" } }, 403),
    ),
  )
  renderPage()
  expect(await screen.findByRole("alertdialog", { name: "Confirm it’s you" })).toBeTruthy()
  expect(screen.queryByRole("button", { name: /Unlink/ })).toBeNull()
})
