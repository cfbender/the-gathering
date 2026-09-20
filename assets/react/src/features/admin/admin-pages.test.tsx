import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { AdminUsersPage } from "./admin-pages"

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
  inserted_at: "2026-09-20T12:00:00Z",
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("admin users", () => {
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
