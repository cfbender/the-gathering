import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { useState } from "react"
import { afterEach, describe, expect, it } from "vite-plus/test"
import { SeatTrackersOverlay } from "./seat-trackers"
import {
  EMPTY_TRACKERS,
  adjustCounter,
  receivedTrackers,
  sharedTrackers,
  type SeatTrackers,
} from "./trackers"

afterEach(cleanup)

const trackers: SeatTrackers = {
  counters: [
    { id: "lands", label: "Lands", value: 7, shared: true },
    { id: "storm", label: "Storm", value: 0, shared: false },
  ],
  effects: [
    {
      id: "a",
      name: "Caretaker's Talent",
      power: 2,
      toughness: 2,
      conditions: ["token"],
      keywords: [],
    },
    {
      id: "b",
      name: "Warren Warleader",
      power: 1,
      toughness: 1,
      conditions: ["attacking"],
      keywords: [],
    },
    {
      id: "c",
      name: "Intangible Virtue",
      power: 1,
      toughness: 1,
      conditions: ["token"],
      keywords: ["vigilance"],
    },
  ],
  shareEffects: true,
}

/** Owns the trackers like `useSeatTrackers` so dialog edits show up in the overlay. */
function LocalOverlay({ initial }: { initial: SeatTrackers }) {
  const [current, setCurrent] = useState(initial)
  return (
    <SeatTrackersOverlay
      trackers={current}
      playerName="Cody"
      local={{
        adjustCounter: (id, delta) => setCurrent((t) => adjustCounter(t, id, delta)),
        setTrackers: setCurrent,
      }}
    />
  )
}

describe("SeatTrackersOverlay", () => {
  it("shows another seat's shared trackers read-only and renders nothing when it shares none", () => {
    // What a peer receives over presence: private counters never leave the owner's browser.
    const received = receivedTrackers(sharedTrackers(trackers))
    const { unmount } = render(<SeatTrackersOverlay trackers={received} playerName="Cody" />)
    const overlay = screen.getByLabelText("Cody's trackers")
    expect(within(overlay).getByLabelText("Lands: 7")).toBeTruthy()
    expect(within(overlay).queryByText("Storm")).toBeNull()
    expect(within(overlay).queryByRole("button")).toBeNull()
    expect(within(overlay).queryByLabelText(/only visible to you/)).toBeNull()
    const rows = within(within(overlay).getByRole("list", { name: "Combat buffs" })).getAllByRole(
      "listitem",
    )
    expect(rows.map((row) => row.textContent)).toEqual([
      "Attacking creatures+1/+1",
      "Token creatures · vigilance+3/+3",
      "Attacking token creatures · vigilance+4/+4",
    ])
    unmount()

    render(<SeatTrackersOverlay trackers={EMPTY_TRACKERS} playerName="Mara" />)
    expect(screen.queryByLabelText("Mara's trackers")).toBeNull()
  })

  it("marks private trackers and adjusts counters for the owner", () => {
    render(<LocalOverlay initial={{ ...trackers, shareEffects: false }} />)
    expect(screen.getByLabelText("Storm is only visible to you")).toBeTruthy()
    expect(screen.getByLabelText("Combat buffs is only visible to you")).toBeTruthy()
    expect(screen.queryByLabelText("Lands is only visible to you")).toBeNull()

    fireEvent.click(screen.getByRole("button", { name: "Increase Lands" }))
    expect(screen.getByLabelText("Lands: 8")).toBeTruthy()
    expect(
      (screen.getByRole("button", { name: "Decrease Storm" }) as HTMLButtonElement).disabled,
    ).toBe(true)
  })

  it("edits counters and buffs in the dialog and recomputes the totals", () => {
    render(<LocalOverlay initial={EMPTY_TRACKERS} />)
    fireEvent.click(screen.getByRole("button", { name: "Trackers" }))
    const dialog = screen.getByRole("dialog", { name: "Trackers" })

    fireEvent.click(within(dialog).getByRole("button", { name: "Add counter" }))
    fireEvent.change(within(dialog).getByLabelText("Counter name"), {
      target: { value: "Lands" },
    })
    fireEvent.change(within(dialog).getByLabelText("Lands value"), { target: { value: "250" } })
    expect((within(dialog).getByLabelText("Lands value") as HTMLInputElement).value).toBe("100")

    fireEvent.click(within(dialog).getByRole("button", { name: "Add buff" }))
    fireEvent.change(within(dialog).getByLabelText("Buff name"), {
      target: { value: "Coat of Arms" },
    })
    fireEvent.change(within(dialog).getByLabelText("Coat of Arms power"), {
      target: { value: "3" },
    })
    fireEvent.click(within(dialog).getByRole("button", { name: "Tokens" }))
    fireEvent.change(within(dialog).getByLabelText("Coat of Arms other conditions"), {
      target: { value: "Elf, Warrior" },
    })
    const totals = within(dialog).getByRole("list", { name: "Combat totals" })
    expect(
      within(totals)
        .getAllByRole("listitem")
        .map((row) => row.textContent),
    ).toEqual(["Elf Warrior token creatures+3/+1"])

    fireEvent.click(within(dialog).getByRole("button", { name: "Close dialog" }))
    const overlay = screen.getByLabelText("Cody's trackers")
    expect(within(overlay).getByLabelText("Lands: 100")).toBeTruthy()
    expect(within(overlay).getByText("Elf Warrior token creatures")).toBeTruthy()
  })
})
