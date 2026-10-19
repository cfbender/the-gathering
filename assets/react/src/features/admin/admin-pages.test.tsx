import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { AdminCatalogPage, AdminUsersPage } from "./admin-pages"

const user = {
  id: 42,
  username: "member",
  display_name: "Member",
  discord_id: null,
  avatar_url: null,
  moxfield_username: null,
  archidekt_username: null,
  manavault_url: null,
  has_manavault_api_key: false,
  has_password: true,
  role: "member",
  disabled: false,
  palette: "claret",
  theme_style: "glass",
  inserted_at: "2026-09-20T12:00:00Z",
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("admin users", () => {
  it("explains deletion and shows the zero-game restriction without removing the account", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init: RequestInit) => {
        const deleting = init.method === "DELETE"
        const body = deleting
          ? { errors: { player: ["must have zero games before deleting this user"] } }
          : { data: url === "/api/admin/users" ? [user] : [] }
        return new Response(JSON.stringify(body), {
          status: deleting ? 422 : 200,
          headers: { "content-type": "application/json" },
        })
      }),
    )
    render(
      <QueryClientProvider client={new QueryClient()}>
        <AdminUsersPage />
      </QueryClientProvider>,
    )
    fireEvent.click(await screen.findByRole("button", { name: "Delete user" }))
    const dialog = screen.getByRole("alertdialog")
    expect(within(dialog).getByText(/linked player must have zero games/)).toBeTruthy()
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete user" }))
    expect(await screen.findByText("must have zero games before deleting this user")).toBeTruthy()
    expect(screen.getByRole("heading", { name: "Member" })).toBeTruthy()
  })

  it("signs the selected user out everywhere", async () => {
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      const body = url === "/api/admin/users" ? { data: [user] } : { data: [] }

      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    })
    vi.stubGlobal("fetch", fetch)
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    })

    render(
      <QueryClientProvider client={queryClient}>
        <AdminUsersPage />
      </QueryClientProvider>,
    )

    fireEvent.click(await screen.findByRole("button", { name: "Sign out everywhere" }))

    await waitFor(() =>
      expect(fetch).toHaveBeenCalledWith(
        "/api/admin/users/42/sessions",
        expect.objectContaining({ method: "DELETE" }),
      ),
    )
    expect(await screen.findByRole("button", { name: "Signed out" })).toBeTruthy()
  })
})

describe("admin catalog", () => {
  it("starts a catalog sync and shows it running", async () => {
    const catalog = {
      status: "succeeded",
      last_started_at: "2026-10-12T04:00:00Z",
      last_finished_at: "2026-10-12T04:05:00Z",
      card_count: 31_000,
      scryfall_updated_at: "2026-10-11T21:00:00Z",
      last_error: null,
    }
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      const body =
        init?.method === "POST"
          ? { data: { status: "started" } }
          : { data: url === "/api/catalog" ? catalog : null }
      return new Response(JSON.stringify(body), {
        status: init?.method === "POST" ? 202 : 200,
        headers: { "content-type": "application/json" },
      })
    })
    vi.stubGlobal("fetch", fetch)
    render(
      <QueryClientProvider client={new QueryClient()}>
        <AdminCatalogPage />
      </QueryClientProvider>,
    )
    expect(await screen.findByText(/Last synced .*31,000 cards/)).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Sync now" }))
    expect(await screen.findByText(/Sync started/)).toBeTruthy()
    expect(screen.getByRole("button", { name: "Syncing…" }).hasAttribute("disabled")).toBe(true)
    expect(fetch).toHaveBeenCalledWith(
      "/api/admin/catalog/sync",
      expect.objectContaining({ method: "POST" }),
    )
  })
})
