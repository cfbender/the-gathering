import { act, cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
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
