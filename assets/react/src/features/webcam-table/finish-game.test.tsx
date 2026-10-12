import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { ToastProvider } from "@/components/ui/toast"
import { FinishGame } from "./finish-game"
import { EMPTY_COUNTERS } from "./seat-counters"
import { EMPTY_TURNS, type TurnState } from "./turns"

const navigate = vi.fn()
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }))

const seat = { ...EMPTY_COUNTERS, life: 40, camera_off: false, eliminated: false, joined_at: 1 }
const participants = [
  { ...seat, peer_id: "peer-a", player_id: 12, player_name: "Alice" },
  { ...seat, peer_id: "peer-b", player_id: 27, player_name: "Bob", eliminated: true },
]

function mount(
  onEndTable: () => Promise<boolean>,
  turns: TurnState = EMPTY_TURNS,
  onRematch: () => Promise<boolean> = () => Promise.resolve(true),
  seats = participants,
) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const onOpenChange = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <FinishGame
          participants={seats}
          playedAt={new Date("2026-09-26T19:00:00Z")}
          timer={{ started_at: null, paused_at: null, paused_ms: 0, server_now: 0 }}
          turns={turns}
          onEndTable={onEndTable}
          onRematch={onRematch}
          onOpenChange={onOpenChange}
        />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return { onOpenChange }
}

const savedGame = () => Promise.resolve(Response.json({ data: { id: 42 } }, { status: 201 }))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  navigate.mockReset()
})

describe("FinishGame", () => {
  it.each(["Record result", "Record and rematch"])(
    "saves named kills, unknown totals and MVP cards with %s",
    async (action) => {
      const posted: unknown[] = []
      vi.stubGlobal(
        "fetch",
        vi.fn((url: string, init?: RequestInit) => {
          if (url.startsWith("/api/cards?"))
            return Promise.resolve(
              Response.json({
                data: [
                  {
                    id: "sol-ring",
                    oracle_id: "sol-ring",
                    name: "Sol Ring",
                    mana_cost: "{1}",
                    type_line: "Artifact",
                    color_identity: [],
                    image_uris: {},
                    can_be_commander: false,
                    commander_pairing: null,
                  },
                ],
              }),
            )
          if (url === "/api/games" && typeof init?.body === "string") {
            posted.push(JSON.parse(init.body))
            return savedGame()
          }
          throw new Error(`Unexpected request: ${url}`)
        }),
      )
      const { onOpenChange } = mount(
        () => Promise.resolve(true),
        EMPTY_TURNS,
        () => Promise.resolve(true),
        [
          ...participants,
          { ...seat, peer_id: "peer-c", player_id: 81, player_name: "Cara", eliminated: true },
        ],
      )
      const alice = within(screen.getByRole("group", { name: "Alice" }))
      const bob = within(screen.getByRole("group", { name: "Bob" }))
      // Number-only kills are not guessed from the winner; naming Bob consumes one of Alice's two.
      expect(alice.getByRole<HTMLInputElement>("spinbutton", { name: "Total kills" }).value).toBe(
        "",
      )
      fireEvent.change(alice.getByRole("spinbutton", { name: "Total kills" }), {
        target: { value: "2" },
      })
      fireEvent.click(alice.getByRole("button", { name: "Kills for Alice" }))
      fireEvent.click(screen.getByRole("checkbox", { name: "Bob" }))
      fireEvent.keyDown(screen.getByRole("checkbox", { name: "Bob" }), { key: "Escape" })
      expect(alice.getByRole<HTMLInputElement>("spinbutton", { name: "Total kills" }).value).toBe(
        "2",
      )
      expect(alice.getByText("1 named · 1 unknown")).toBeTruthy()
      fireEvent.change(bob.getByRole("spinbutton", { name: "Total kills" }), {
        target: { value: "0" },
      })
      fireEvent.change(alice.getByRole("combobox", { name: "MVP card (optional)" }), {
        target: { value: "Sol" },
      })
      fireEvent.click(await screen.findByRole("option", { name: /Sol Ring/ }))
      if (action === "Record and rematch")
        fireEvent.click(screen.getByRole("radio", { name: /End and rematch/ }))
      fireEvent.click(screen.getByRole("button", { name: action }))
      await waitFor(() => expect(posted).toHaveLength(1))
      expect(posted[0]).toMatchObject({
        seats: [
          {
            player_id: 12,
            result: "win",
            kills: 2,
            eliminated_by_player_id: null,
            mvp_card_id: "sol-ring",
            mvp_card_name: "Sol Ring",
          },
          {
            player_id: 27,
            result: "loss",
            kills: 0,
            eliminated_by_player_id: 12,
            mvp_card_id: null,
            mvp_card_name: null,
          },
          {
            player_id: 81,
            result: "loss",
            kills: null,
            eliminated_by_player_id: null,
            mvp_card_id: null,
            mvp_card_name: null,
          },
        ],
      })
      await waitFor(() =>
        action === "Record result"
          ? expect(navigate).toHaveBeenCalledWith({
              to: "/games/$gameId",
              params: { gameId: "42" },
            })
          : expect(onOpenChange).toHaveBeenCalledWith(false),
      )
    },
  )

  it("shows seat validation errors without discarding entered totals", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() =>
        Promise.resolve(
          Response.json(
            {
              errors: { seats: [{ kills: ["must include all named victims"] }] },
            },
            { status: 422 },
          ),
        ),
      ),
    )
    const onEndTable = vi.fn(() => Promise.resolve(true))
    mount(onEndTable)
    const alice = within(screen.getByRole("group", { name: "Alice" }))
    fireEvent.change(alice.getByRole("spinbutton", { name: "Total kills" }), {
      target: { value: "2" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Record result" }))
    expect(await screen.findByText("Seat 1: kills must include all named victims")).toBeTruthy()
    expect(alice.getByRole<HTMLInputElement>("spinbutton", { name: "Total kills" }).value).toBe("2")
    expect(onEndTable).not.toHaveBeenCalled()
    expect(navigate).not.toHaveBeenCalled()
  })

  it("ends the table without posting a game once confirmed", async () => {
    const fetch = vi.fn()
    vi.stubGlobal("fetch", fetch)
    const onEndTable = vi.fn(() => Promise.resolve(true))
    mount(onEndTable)

    fireEvent.click(screen.getByRole("button", { name: "End without recording" }))
    expect(onEndTable).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
    fireEvent.click(screen.getByRole("button", { name: "End without recording" }))
    expect(screen.getByRole("alert").textContent).toContain("without adding it to history")
    fireEvent.click(screen.getByRole("button", { name: "End without recording" }))

    await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/games" }))
    expect(onEndTable).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })

  it("stays open with an error when the table could not be ended", async () => {
    mount(() => Promise.resolve(false))

    fireEvent.click(screen.getByRole("button", { name: "End without recording" }))
    fireEvent.click(screen.getByRole("button", { name: "End without recording" }))

    expect(await screen.findByText(/Could not end the game/)).toBeTruthy()
    expect(navigate).not.toHaveBeenCalled()
  })

  it("records the result, then ends the table and opens the saved game", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve(Response.json({ data: { id: 42 } }, { status: 201 }))),
    )
    const onEndTable = vi.fn(() => Promise.resolve(true))
    mount(onEndTable)

    fireEvent.click(screen.getByRole("button", { name: "Record result" }))

    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({ to: "/games/$gameId", params: { gameId: "42" } }),
    )
    expect(onEndTable).toHaveBeenCalledOnce()
  })

  it("records the highest turn any player reached as the game's turns", async () => {
    const fetch = vi.fn((_url: string, _init: RequestInit) =>
      Promise.resolve(Response.json({ data: { id: 42 } }, { status: 201 })),
    )
    vi.stubGlobal("fetch", fetch)
    mount(() => Promise.resolve(true), { ...EMPTY_TURNS, counts: { 12: 9, 27: 8 } })

    expect(screen.getByRole<HTMLInputElement>("spinbutton", { name: "Turns" }).value).toBe("9")
    fireEvent.click(screen.getByRole("button", { name: "Record result" }))

    await waitFor(() => expect(fetch).toHaveBeenCalledOnce())
    expect(JSON.parse(fetch.mock.calls[0]![1].body as string).turns).toBe(9)
  })

  it("records the result, then resets the room for a rematch without navigating", async () => {
    const fetch = vi.fn(savedGame)
    vi.stubGlobal("fetch", fetch)
    const onEndTable = vi.fn(() => Promise.resolve(true))
    const onRematch = vi.fn(() => Promise.resolve(true))
    const { onOpenChange } = mount(onEndTable, EMPTY_TURNS, onRematch)

    fireEvent.click(screen.getByRole("radio", { name: /End and rematch/ }))
    expect(screen.queryByRole("button", { name: "Record result" })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Record and rematch" }))

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
    expect(fetch).toHaveBeenCalledOnce()
    expect(onRematch).toHaveBeenCalledOnce()
    expect(onEndTable).not.toHaveBeenCalled()
    expect(navigate).not.toHaveBeenCalled()
    // The open dialog hides the toast stack from the accessibility tree, so find it by text.
    expect(screen.getByText(/rematch is ready/)).toBeTruthy()
  })

  it("closes after recording even when the rematch fails, so the game is not recorded twice", async () => {
    vi.stubGlobal("fetch", vi.fn(savedGame))
    const { onOpenChange } = mount(vi.fn(), EMPTY_TURNS, () => Promise.resolve(false))

    fireEvent.click(screen.getByRole("radio", { name: /End and rematch/ }))
    fireEvent.click(screen.getByRole("button", { name: "Record and rematch" }))

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
    expect(screen.getByText(/rematch did not start/)).toBeTruthy()
    expect(navigate).not.toHaveBeenCalled()
  })

  it("starts a rematch without posting a game once confirmed", async () => {
    const fetch = vi.fn()
    vi.stubGlobal("fetch", fetch)
    const onEndTable = vi.fn(() => Promise.resolve(true))
    const onRematch = vi.fn(() => Promise.resolve(true))
    const { onOpenChange } = mount(onEndTable, EMPTY_TURNS, onRematch)

    fireEvent.click(screen.getByRole("radio", { name: /End and rematch/ }))
    fireEvent.click(screen.getByRole("button", { name: "Rematch without recording" }))
    expect(onRematch).not.toHaveBeenCalled()
    expect(screen.getByRole("alert").textContent).toContain("rematch for everyone")
    fireEvent.click(screen.getByRole("button", { name: "Rematch without recording" }))

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
    expect(onRematch).toHaveBeenCalledOnce()
    expect(onEndTable).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
    expect(navigate).not.toHaveBeenCalled()
  })

  it("stays open with an error when the rematch could not start", async () => {
    const { onOpenChange } = mount(vi.fn(), EMPTY_TURNS, () => Promise.resolve(false))

    fireEvent.click(screen.getByRole("radio", { name: /End and rematch/ }))
    fireEvent.click(screen.getByRole("button", { name: "Rematch without recording" }))
    fireEvent.click(screen.getByRole("button", { name: "Rematch without recording" }))

    expect(await screen.findByText(/Could not start the rematch/)).toBeTruthy()
    expect(onOpenChange).not.toHaveBeenCalled()
    expect(navigate).not.toHaveBeenCalled()
  })
})
