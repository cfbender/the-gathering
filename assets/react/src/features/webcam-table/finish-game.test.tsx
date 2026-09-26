import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { FinishGame } from "./finish-game"
import { EMPTY_COUNTERS } from "./seat-counters"

const navigate = vi.fn()
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }))

const seat = { ...EMPTY_COUNTERS, life: 40, camera_off: false, eliminated: false, joined_at: 1 }
const participants = [
  { ...seat, peer_id: "peer-a", player_id: 12, player_name: "Alice" },
  { ...seat, peer_id: "peer-b", player_id: 27, player_name: "Bob", eliminated: true },
]

function mount(onEndTable: () => Promise<boolean>) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <FinishGame
        participants={participants}
        playedAt={new Date("2026-09-26T19:00:00Z")}
        timer={{ started_at: null, paused_at: null, paused_ms: 0, server_now: 0 }}
        onEndTable={onEndTable}
        onOpenChange={vi.fn()}
      />
    </QueryClientProvider>,
  )
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  navigate.mockReset()
})

describe("FinishGame", () => {
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
})
