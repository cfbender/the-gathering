import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { Game } from "./games"
import { DeleteGame } from "./delete-game"

const navigate = vi.fn()
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }))

const game = {
  id: 42,
  played_at: "2026-09-19T18:00:00Z",
  duration_minutes: null,
  turns: null,
  win_condition: null,
  notes: null,
  source: "manual",
  external_id: null,
  created_by_user_id: 1,
  seats: [],
} satisfies Game

function mount() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const invalidations = vi.spyOn(client, "invalidateQueries")
  render(
    <QueryClientProvider client={client}>
      <DeleteGame game={game} />
    </QueryClientProvider>,
  )
  return invalidations
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  navigate.mockReset()
})

describe("DeleteGame", () => {
  it("does not delete when confirmation is cancelled", () => {
    const fetch = vi.fn()
    vi.stubGlobal("fetch", fetch)
    mount()

    fireEvent.click(screen.getByRole("button", { name: "Delete game" }))
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))

    expect(screen.queryByRole("alertdialog")).toBeNull()
    expect(fetch).not.toHaveBeenCalled()
  })

  it("deletes, invalidates dependent caches, and navigates to the games list", async () => {
    let resolveDelete!: (response: Response) => void
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>((resolve) => (resolveDelete = resolve))),
    )
    const invalidations = mount()

    fireEvent.click(screen.getByRole("button", { name: "Delete game" }))
    fireEvent.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete game" }),
    )
    expect(screen.getByRole("button", { name: "Deleting…" }).hasAttribute("disabled")).toBe(true)
    expect(screen.getByRole("status").textContent).toBe("Deleting game")

    await waitFor(() => expect(resolveDelete).toBeTypeOf("function"))
    resolveDelete(new Response(null, { status: 204 }))
    await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/games", search: {} }))
    expect(fetch).toHaveBeenCalledWith(
      "/api/games/42",
      expect.objectContaining({ method: "DELETE" }),
    )
    expect(invalidations.mock.calls.map(([options]) => options?.queryKey)).toEqual([
      ["games"],
      ["players"],
      ["decks"],
      ["stats"],
    ])
  })

  it("keeps the user on the page and announces an API error", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(JSON.stringify({ errors: { detail: "You cannot delete this game." } }), {
            status: 403,
            headers: { "content-type": "application/json" },
          }),
      ),
    )
    mount()

    fireEvent.click(screen.getByRole("button", { name: "Delete game" }))
    fireEvent.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete game" }),
    )

    expect((await screen.findByRole("alert")).textContent).toContain("You cannot delete this game.")
    expect(navigate).not.toHaveBeenCalled()
    expect(screen.getByRole("button", { name: "Delete game" }).hasAttribute("disabled")).toBe(false)
  })
})
