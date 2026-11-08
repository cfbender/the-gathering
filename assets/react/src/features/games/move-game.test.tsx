import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { Game } from "./games"
import { MoveGame } from "./move-game"

const navigate = vi.fn()
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }))

const game = {
  id: 42,
  format: "commander",
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

const admin = {
  id: 1,
  username: "admin",
  display_name: "Admin",
  role: "admin",
  has_password: true,
  playgroup_id: 1,
}

const playgroups = [
  { id: 1, name: "Main", discord_guild_id: null, invite_enabled: false },
  { id: 2, name: "Thursday crew", discord_guild_id: "123", invite_enabled: false },
]

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

/** Serves the session and playgroup lists; `onMove` answers the move request. */
function stubFetch(
  onMove: (init?: RequestInit) => Promise<Response>,
  user: Record<string, unknown> = admin,
) {
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === "/api/session") return json({ data: user })
    if (url === "/api/playgroups") return json({ data: playgroups })
    if (url === "/api/admin/games/42/move") return onMove(init)
    throw new Error(`Unexpected request: ${url}`)
  })
  vi.stubGlobal("fetch", fetch)
  return fetch
}

function mount() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const invalidations = vi.spyOn(client, "invalidateQueries")
  render(
    <QueryClientProvider client={client}>
      <MoveGame game={game} />
    </QueryClientProvider>,
  )
  return invalidations
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  navigate.mockReset()
})

describe("MoveGame", () => {
  it("is hidden from members", async () => {
    const fetch = stubFetch(() => Promise.reject(new Error("unexpected")), {
      ...admin,
      role: "member",
    })
    mount()

    await waitFor(() => expect(fetch).toHaveBeenCalledWith("/api/session", expect.anything()))
    expect(screen.queryByRole("heading", { name: "Move to another playgroup" })).toBeNull()
    expect(fetch).not.toHaveBeenCalledWith("/api/playgroups", expect.anything())
  })

  it("offers only the other playgroups and does nothing when cancelled", async () => {
    const fetch = stubFetch(() => Promise.reject(new Error("unexpected")))
    mount()

    const select = await screen.findByRole("combobox", { name: "Playgroup to move into" })
    expect(
      within(select)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Choose a playgroup…", "Thursday crew"])
    expect(screen.getByRole("button", { name: "Move" }).hasAttribute("disabled")).toBe(true)

    fireEvent.change(select, { target: { value: "2" } })
    fireEvent.click(screen.getByRole("button", { name: "Move" }))
    expect(screen.getByRole("dialog").textContent).toContain("Move this game to Thursday crew?")
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))

    expect(screen.queryByRole("dialog")).toBeNull()
    expect(fetch).not.toHaveBeenCalledWith("/api/admin/games/42/move", expect.anything())
  })

  it("moves the game, invalidates dependent caches, and navigates to the games list", async () => {
    let body: unknown
    const fetch = stubFetch(async (init) => {
      body = JSON.parse(typeof init?.body === "string" ? init.body : "{}")
      return json({ data: game })
    })
    const invalidations = mount()

    fireEvent.change(await screen.findByRole("combobox", { name: "Playgroup to move into" }), {
      target: { value: "2" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Move" }))
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Move game" }))

    await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/games", search: {} }))
    expect(fetch).toHaveBeenCalledWith(
      "/api/admin/games/42/move",
      expect.objectContaining({ method: "POST" }),
    )
    expect(body).toEqual({ playgroup_id: 2 })
    expect(invalidations.mock.calls.map(([options]) => options?.queryKey)).toEqual([
      ["games"],
      ["players"],
      ["decks"],
      ["stats"],
    ])
  })

  it("shows the seat conflict the server refused the move for", async () => {
    stubFetch(async () =>
      json({ errors: { seats: ["Bob is a different player in that playgroup"] } }, 422),
    )
    mount()

    fireEvent.change(await screen.findByRole("combobox", { name: "Playgroup to move into" }), {
      target: { value: "2" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Move" }))
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Move game" }))

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Bob is a different player in that playgroup",
    )
    expect(navigate).not.toHaveBeenCalled()
    expect(screen.getByRole("button", { name: "Move" }).hasAttribute("disabled")).toBe(false)
  })
})
