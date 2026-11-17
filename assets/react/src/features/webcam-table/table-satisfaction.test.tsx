import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { EMPTY_COUNTERS } from "./seat-counters"
import { TableSatisfactionPrompt } from "./table-satisfaction"

afterEach(cleanup)

const seat = {
  ...EMPTY_COUNTERS,
  peer_id: "peer-a",
  player_id: 12,
  player_name: "Alice",
  life: 0,
  camera_off: false,
  eliminated: true,
  joined_at: 1,
}

function mount(overrides: Partial<Parameters<typeof TableSatisfactionPrompt>[0]> = {}) {
  const onRate = vi.fn()
  render(
    <TableSatisfactionPrompt
      participant={seat}
      spectating={false}
      mode="commander"
      unratedGame={null}
      onRate={onRate}
      onClearUnratedGame={() => {}}
      {...overrides}
    />,
  )
  return { onRate }
}

it("asks a knocked-out seat once and rates with a single click, worst face first", () => {
  const { onRate } = mount()
  expect(screen.getByRole("dialog", { name: /You're out/ })).toBeTruthy()
  expect(
    screen.getAllByRole("button", { name: /awful|bad|meh|good|rad/ }).map((b) => b.title),
  ).toEqual(["awful", "bad", "meh", "good", "rad"])
  fireEvent.click(screen.getByRole("button", { name: "good" }))
  expect(onRate).toHaveBeenCalledExactlyOnceWith(4)
  expect(screen.queryByRole("dialog")).toBeNull()
})

it("stays away once dismissed, and for seats still in, already rated, or unrecorded modes", () => {
  const { onRate } = mount()
  fireEvent.click(screen.getByRole("button", { name: "Not now" }))
  expect(screen.queryByRole("dialog")).toBeNull()
  expect(onRate).not.toHaveBeenCalled()
  for (const props of [
    { participant: { ...seat, eliminated: false } },
    { participant: { ...seat, satisfaction: 3 } },
    { mode: "standard" as const },
    { spectating: true },
  ]) {
    cleanup()
    mount(props)
    expect(screen.queryByRole("dialog")).toBeNull()
  }
})
