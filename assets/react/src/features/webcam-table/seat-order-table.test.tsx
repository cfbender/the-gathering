import { act, cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import type { TimerSample } from "./game-timer"
import { SeatOrderTable, type SeatOrderTableProps } from "./seat-order-table"
import { EMPTY_COUNTERS } from "./seat-counters"
import { EMPTY_TURNS } from "./turns"

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it("animates new shuffle events once, never replaying on tab remount (even mid-animation)", () => {
  vi.useFakeTimers()
  vi.stubGlobal("matchMedia", () => ({ matches: false }))
  const alice = {
    ...EMPTY_COUNTERS,
    player_id: 1,
    peer_id: "a",
    player_name: "Alice",
    life: 40,
    camera_off: false,
    eliminated: false,
    joined_at: 1,
  }
  const bob = { ...alice, player_id: 2, peer_id: "b", player_name: "Bob" }
  const props: SeatOrderTableProps = {
    participants: [alice, bob],
    localParticipant: alice,
    decks: [],
    shuffleVersion: 0,
    turns: EMPTY_TURNS,
    timer: null,
    onSetEliminated: vi.fn(),
    onAdjustTurn: vi.fn(),
  }
  let view = render(<SeatOrderTable {...props} />)
  const busy = () => screen.getByRole("table", { name: "Turn order" }).getAttribute("aria-busy")
  expect(busy()).toBe("false")
  view.rerender(<SeatOrderTable {...props} participants={[bob, alice]} shuffleVersion={1} />)
  expect(busy()).toBe("true")
  act(() => {
    vi.advanceTimersByTime(960)
  })
  expect(busy()).toBe("false")
  expect(screen.getAllByRole("row")[1]?.getAttribute("data-peer")).toBe("b")
  view.unmount()
  view = render(<SeatOrderTable {...props} shuffleVersion={1} />)
  expect(busy()).toBe("false")
  view.rerender(<SeatOrderTable {...props} shuffleVersion={2} />)
  expect(busy()).toBe("true")
  view.unmount()
  view = render(<SeatOrderTable {...props} shuffleVersion={2} />)
  expect(busy()).toBe("false")
  view.unmount()
  render(<SeatOrderTable {...props} shuffleVersion={3} />)
  expect(busy()).toBe("false")
})

function seats() {
  const alice = {
    ...EMPTY_COUNTERS,
    player_id: 1,
    peer_id: "a",
    player_name: "Alice",
    life: 40,
    camera_off: false,
    eliminated: false,
    joined_at: 1,
  }
  const bob = { ...alice, player_id: 2, peer_id: "b", player_name: "Bob" }
  const props: SeatOrderTableProps = {
    participants: [alice, bob],
    localParticipant: alice,
    decks: [],
    shuffleVersion: 0,
    turns: EMPTY_TURNS,
    timer: null,
    onSetEliminated: vi.fn(),
    onAdjustTurn: vi.fn(),
    onMoveSeat: vi.fn(),
  }
  return props
}

const started: TimerSample = {
  state: { started_at: 1, paused_at: null, paused_ms: 0, server_now: 1 },
  receivedAt: 1,
}

it("shows reorder arrows to the owner and disables the ends of the order", () => {
  const props = seats()
  render(<SeatOrderTable {...props} />)
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Move Alice up" }).disabled).toBe(
    true,
  )
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Move Bob down" }).disabled).toBe(
    true,
  )
  screen.getByRole("button", { name: "Move Alice down" }).click()
  expect(props.onMoveSeat).toHaveBeenCalledWith("a", 1)
  screen.getByRole("button", { name: "Move Bob up" }).click()
  expect(props.onMoveSeat).toHaveBeenCalledWith("b", -1)
})

it("hides the arrows from non-owners", () => {
  render(<SeatOrderTable {...seats()} readOnly />)
  expect(screen.queryByRole("button", { name: /^Move / })).toBeNull()
})

it("keeps arrows mid-game in Commander but locks team formats once started", () => {
  const props = seats()
  const view = render(<SeatOrderTable {...props} mode="commander" timer={started} />)
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Move Bob up" }).disabled).toBe(
    false,
  )
  view.rerender(<SeatOrderTable {...props} mode="two_headed_giant" timer={started} />)
  expect(screen.queryByRole("button", { name: /^Move / })).toBeNull()
  view.rerender(<SeatOrderTable {...props} mode="two_headed_giant" timer={null} />)
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Move Bob up" }).disabled).toBe(
    false,
  )
  view.rerender(<SeatOrderTable {...props} mode="five_star" timer={started} />)
  expect(screen.queryByRole("button", { name: /^Move / })).toBeNull()
})
