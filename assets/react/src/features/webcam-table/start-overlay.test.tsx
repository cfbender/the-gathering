import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import type { GameTimerState } from "./game-timer"
import type { TableParticipant } from "./room-types"
import { EMPTY_COUNTERS } from "./seat-counters"
import { StartOverlay } from "./start-overlay"
import type { TableView } from "./table-view"
import { EMPTY_TURNS } from "./turns"

afterEach(cleanup)

const alice: TableParticipant = {
  ...EMPTY_COUNTERS,
  player_id: 1,
  peer_id: "a",
  player_name: "Alice",
  life: 40,
  camera_off: false,
  eliminated: false,
  joined_at: 1,
}
const bob: TableParticipant = { ...alice, player_id: 2, peer_id: "b", player_name: "Bob" }
const mulligan: GameTimerState = {
  started_at: 1000,
  paused_at: 1000,
  paused_ms: 0,
  server_now: 5000,
}

function view(local: TableParticipant, state: GameTimerState, beginPlay = vi.fn()) {
  return {
    seated: [alice, bob],
    localParticipant: local,
    room: {
      mode: "commander",
      spectating: false,
      timer: { state, receivedAt: 0 },
      turns: { ...EMPTY_TURNS, active_player_id: alice.player_id },
      beginPlay,
    },
  } as unknown as TableView
}

it("gives the first player a Start button that begins play", () => {
  const beginPlay = vi.fn()
  render(<StartOverlay view={view(alice, mulligan, beginPlay)} />)
  fireEvent.click(screen.getByRole("button", { name: "Start" }))
  expect(beginPlay).toHaveBeenCalledOnce()
})

it("tells everyone else who the table is waiting on", () => {
  render(<StartOverlay view={view(bob, mulligan)} />)
  expect(screen.queryByRole("button", { name: "Start" })).toBeNull()
  expect(screen.getByRole("status").textContent).toContain("waiting for Alice to start")
})

it("disappears once the clock runs", () => {
  const { container } = render(
    <StartOverlay view={view(alice, { ...mulligan, paused_at: null })} />,
  )
  expect(container.innerHTML).toBe("")
})
