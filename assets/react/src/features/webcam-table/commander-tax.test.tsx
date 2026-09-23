import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render as renderBare, screen } from "@testing-library/react"
import { useState, type ReactElement } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { DeckSummary } from "@/features/decks/decks"
import { CommanderTax } from "./commander-tax"
import { EMPTY_COUNTERS, changeCounter } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

afterEach(cleanup)

/** CommanderPicker's create/edit actions read the session and players queries. */
function render(ui: ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  client.setQueryData(["session"], { id: 7, role: "member" })
  client.setQueryData(["players"], [{ id: 1, user_id: 7 }])
  return renderBare(<QueryClientProvider client={client}>{ui}</QueryClientProvider>)
}

const deck: DeckSummary = {
  id: 7,
  player_id: 1,
  name: "Partners",
  commander_name: "Tymna",
  partner_name: "Thrasios",
  commander_card_id: null,
  partner_card_id: null,
  commander_art_crop_url: "/tymna.jpg",
  partner_art_crop_url: "/thrasios.jpg",
  color_identity: "WUBG",
  decklist_url: null,
  decklist_source: null,
  archived_at: null,
  skip_count: 0,
  included_for_play: true,
  player: null,
}
const seat: TableParticipant = {
  ...EMPTY_COUNTERS,
  peer_id: "a",
  player_id: 1,
  player_name: "Alice",
  deck_id: 7,
  life: 40,
  camera_off: false,
  joined_at: 1000,
}

describe("inline commander tax", () => {
  it("uses each partner's art and changes only that commander's tax with click, minus, and right-click", () => {
    function Seat() {
      const [participant, setParticipant] = useState(seat)
      return (
        <CommanderTax
          participant={participant}
          decks={[deck]}
          local
          onChooseDeck={() => {}}
          onAdjust={(counter, delta) =>
            setParticipant((previous) => ({
              ...previous,
              ...changeCounter(previous, counter, delta),
            }))
          }
        />
      )
    }
    render(<Seat />)
    expect(
      screen
        .getByRole("button", { name: "Tymna commander tax: 0" })
        .querySelector("img")
        ?.getAttribute("src"),
    ).toBe("/tymna.jpg")
    expect(
      screen
        .getByRole("button", { name: "Thrasios commander tax: 0" })
        .querySelector("img")
        ?.getAttribute("src"),
    ).toBe("/thrasios.jpg")
    fireEvent.click(screen.getByRole("button", { name: "Tymna commander tax: 0" }))
    fireEvent.click(screen.getByRole("button", { name: "Tymna commander tax: 2" }))
    fireEvent.click(screen.getByRole("button", { name: "Thrasios commander tax: 0" }))
    expect(screen.getByRole("button", { name: "Tymna commander tax: 4" }).textContent).toBe("4")
    expect(screen.getByRole("button", { name: "Thrasios commander tax: 2" }).textContent).toBe("2")
    fireEvent.contextMenu(screen.getByRole("button", { name: "Tymna commander tax: 4" }))
    fireEvent.click(screen.getByRole("button", { name: "Decrease Tymna commander tax" }))
    fireEvent.contextMenu(screen.getByRole("button", { name: "Tymna commander tax: 0" }))
    expect(
      screen.getByRole("button", { name: "Decrease Tymna commander tax" }).hasAttribute("disabled"),
    ).toBe(true)
    expect(screen.getByRole("button", { name: "Thrasios commander tax: 2" })).toBeTruthy()
  })

  it("keeps remote counters read-only", () => {
    const onAdjust = vi.fn()
    render(
      <CommanderTax
        participant={{ ...seat, commander_casts: { Tymna: 3, Thrasios: 1 } }}
        decks={[deck]}
        local={false}
        onChooseDeck={() => {}}
        onAdjust={onAdjust}
      />,
    )
    const thumbnail = screen.getByRole("button", { name: "Tymna commander tax: 6" })
    fireEvent.click(thumbnail)
    fireEvent.contextMenu(thumbnail)
    expect(thumbnail.hasAttribute("disabled")).toBe(true)
    expect(screen.queryByRole("button", { name: "Decrease Tymna commander tax" })).toBeNull()
    expect(onAdjust).not.toHaveBeenCalled()
  })

  it("allows correction at the server limit and renders missing/broken art without hiding tax", () => {
    const onAdjust = vi.fn()
    render(
      <CommanderTax
        participant={{ ...seat, commander_casts: { Tymna: 999 } }}
        decks={[{ ...deck, partner_art_crop_url: null }]}
        local
        onChooseDeck={() => {}}
        onAdjust={onAdjust}
      />,
    )
    const thumbnail = screen.getByRole("button", { name: "Tymna commander tax: 1998" })
    fireEvent.error(thumbnail.querySelector("img")!)
    expect(thumbnail.querySelector("img")).toBeNull()
    expect(thumbnail.textContent).toBe("1998")
    expect(
      screen.getByRole("button", { name: "Thrasios commander tax: 0" }).querySelector("img"),
    ).toBeNull()
    fireEvent.click(thumbnail)
    expect(onAdjust).not.toHaveBeenCalled()
    fireEvent.contextMenu(thumbnail)
    expect(onAdjust).toHaveBeenCalledWith({ kind: "casts", commander: "Tymna" }, -1)
  })

  it("keeps deck selection on the name control, including before a deck is selected", () => {
    const onChooseDeck = vi.fn()
    render(
      <CommanderTax
        participant={{ ...seat, deck_id: undefined }}
        decks={[deck]}
        local
        onChooseDeck={onChooseDeck}
        onAdjust={() => {}}
      />,
    )
    expect(screen.queryByRole("button", { name: /commander tax/ })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Choose Alice's commander" }))
    fireEvent.click(screen.getByText("Partners"))
    expect(onChooseDeck).toHaveBeenCalledWith(7)
  })
})
