import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  act,
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  within,
} from "@testing-library/react"
import { useState, type ReactElement } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { DeckSummary } from "@/features/decks/decks"
import { CameraTile } from "./board"
import { CommanderControl } from "./commander-control"
import { LifeControl } from "./life-control"
import { SeatBar } from "./seat-bar"
import { SeatCounterControls } from "./seat-counter-controls"
import { EMPTY_COUNTERS, changeCounter } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

afterEach(cleanup)

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
  commander_art_crop_url: null,
  partner_art_crop_url: null,
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
  life: 37,
  camera_off: false,
  eliminated: false,
  joined_at: 1000,
}
const opponent = { ...seat, peer_id: "b", player_id: 2, player_name: "Bob" }

it("keeps life interactions separate from selecting the camera board", () => {
  const onActivate = vi.fn()
  const onChangeLife = vi.fn()
  render(
    <CameraTile
      participant={seat}
      local
      active={false}
      onActivate={onActivate}
      lifeControl={
        <LifeControl
          life={37}
          local
          size="tile"
          counters={() => null}
          onChangeLife={onChangeLife}
        />
      }
    />,
  )
  fireEvent.focus(screen.getByRole("textbox", { name: /edit life total/ }))
  fireEvent.click(screen.getByRole("button", { name: "Gain 1 life" }))
  expect(onChangeLife).toHaveBeenCalledWith(1)
  expect(onActivate).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole("button", { name: "Show Alice's board" }))
  expect(onActivate).toHaveBeenCalledOnce()
})

describe.each(["tile", "board"] as const)("%s life control", (size) => {
  it("reveals life buttons on hover and focus, keeps them while moving focus, and sends signed deltas", () => {
    const onChangeLife = vi.fn()
    render(
      <LifeControl life={37} local size={size} counters={() => null} onChangeLife={onChangeLife} />,
    )
    const box = screen.getByRole("textbox", { name: "37 life; edit life total" })
    expect(screen.queryByRole("button", { name: "Gain 1 life" })).toBeNull()
    fireEvent.mouseEnter(box)
    fireEvent.click(screen.getByRole("button", { name: "Gain 1 life" }))
    fireEvent.click(screen.getByRole("button", { name: "Lose 1 life" }))
    expect(onChangeLife.mock.calls).toEqual([[1], [-1]])
    fireEvent.mouseLeave(box)
    expect(screen.queryByRole("button", { name: "Gain 1 life" })).toBeNull()
    fireEvent.focus(box)
    const gain = screen.getByRole("button", { name: "Gain 1 life" })
    fireEvent.blur(box, { relatedTarget: gain })
    expect(screen.getByRole("button", { name: "Lose 1 life" })).toBeTruthy()
    fireEvent.blur(gain, { relatedTarget: document.body })
    expect(screen.queryByRole("button", { name: "Lose 1 life" })).toBeNull()
  })

  it("commits a typed total as one delta on Enter or blur, and discards on Escape or junk", () => {
    const onChangeLife = vi.fn()
    render(
      <LifeControl life={37} local size={size} counters={() => null} onChangeLife={onChangeLife} />,
    )
    const box = screen.getByRole("textbox", { name: "37 life; edit life total" })
    fireEvent.change(box, { target: { value: "25" } })
    fireEvent.keyDown(box, { key: "Enter" })
    fireEvent.blur(box)
    fireEvent.change(box, { target: { value: "-3" } })
    fireEvent.blur(box)
    fireEvent.change(box, { target: { value: "5000" } })
    fireEvent.blur(box)
    expect(onChangeLife.mock.calls).toEqual([[-12], [-40], [962]])
    fireEvent.change(box, { target: { value: "12" } })
    fireEvent.keyDown(box, { key: "Escape" })
    fireEvent.blur(box)
    for (const junk of ["", "abc", "37"]) {
      fireEvent.change(box, { target: { value: junk } })
      fireEvent.blur(box)
    }
    expect(onChangeLife).toHaveBeenCalledTimes(3)
    expect((box as HTMLInputElement).value).toBe("37")
  })

  it("never renders remote life buttons even on hover", () => {
    render(
      <LifeControl
        life={37}
        local={false}
        size={size}
        counters={() => <button>Counters</button>}
        onChangeLife={vi.fn()}
      />,
    )
    fireEvent.mouseEnter(screen.getByLabelText("37 life"))
    expect(screen.queryByRole("button", { name: /life/, hidden: true })).toBeNull()
    expect(screen.queryByRole("textbox")).toBeNull()
    expect(screen.getByRole("button", { name: "Counters" })).toBeTruthy()
  })
})

describe("seat counters", () => {
  it("shows partner art and unrevealed placeholders, and applies only pending life once", () => {
    vi.useFakeTimers()
    const onChangeLife = vi.fn()
    function Seat() {
      const [participant, setParticipant] = useState(seat)
      return (
        <SeatCounterControls
          participant={participant}
          participants={[
            participant,
            opponent,
            { ...opponent, player_id: 3, player_name: "Cara", deck_id: undefined },
          ]}
          decks={[
            {
              ...deck,
              commander_art_crop_url: "/api/card-images/tymna",
              partner_art_crop_url: "/api/card-images/thrasios",
            },
          ]}
          local
          monarch={false}
          onTakeMonarch={vi.fn()}
          onChangeLife={onChangeLife}
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
    fireEvent.click(screen.getByRole("button", { name: "Alice's counters" }))
    expect(screen.getByRole("img", { name: "Bob · Tymna" }).getAttribute("src")).toBe(
      "/api/card-images/tymna",
    )
    expect(screen.getByRole("img", { name: "Bob · Thrasios" }).getAttribute("src")).toBe(
      "/api/card-images/thrasios",
    )
    expect(screen.queryByText("Bob · Tymna")).toBeNull()
    expect(screen.getByText("Cara · Commander not revealed")).toBeTruthy()
    expect(screen.getByRole("img", { name: "No image available for Cara" })).toBeTruthy()
    const increase = screen.getByRole("button", { name: "Increase Bob · Tymna" })
    for (let i = 0; i < 3; i++) fireEvent.click(increase)
    fireEvent.click(screen.getByRole("button", { name: "Increase Bob · Thrasios" }))
    expect(onChangeLife).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Apply -3 life for Bob · Tymna" }))
    expect(onChangeLife.mock.calls).toEqual([[-3]])
    expect(screen.queryByRole("button", { name: "Apply -3 life for Bob · Tymna" })).toBeNull()
    expect(screen.getByRole("button", { name: "Apply -1 life for Bob · Thrasios" })).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Decrease Bob · Tymna" }))
    fireEvent.click(screen.getByRole("button", { name: "Apply +1 life for Bob · Tymna" }))
    expect(onChangeLife.mock.calls).toEqual([[-3], [1]])
    act(() => {
      vi.advanceTimersByTime(5000)
    })
    expect(screen.queryByRole("button", { name: /Apply .* life/ })).toBeNull()
    fireEvent.click(increase)
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })
    fireEvent.click(screen.getByRole("button", { name: "Alice's counters" }))
    expect(screen.queryByRole("button", { name: /Apply .* life/ })).toBeNull()
    vi.useRealTimers()
  })

  it("uses ±1 casts for ±2 tax per partner, ±1 poison/rad/damage, and keeps the panel open", () => {
    const onAdjust = vi.fn()
    const onTakeMonarch = vi.fn()
    function Seat() {
      const [participant, setParticipant] = useState<TableParticipant>({
        ...seat,
        poison: 3,
        commander_casts: { Tymna: 2, Thrasios: 1 },
      })
      return (
        <LifeControl
          life={37}
          local
          size="tile"
          onChangeLife={vi.fn()}
          counters={(onOpenChange) => (
            <SeatCounterControls
              participant={participant}
              participants={[participant, opponent]}
              decks={[deck]}
              local
              monarch={false}
              onOpenChange={onOpenChange}
              onTakeMonarch={onTakeMonarch}
              onChangeLife={vi.fn()}
              onAdjust={(counter, delta) => {
                onAdjust(counter, delta)
                setParticipant((previous) => ({
                  ...previous,
                  ...changeCounter(previous, counter, delta),
                }))
              }}
            />
          )}
        />
      )
    }
    render(<Seat />)
    fireEvent.focus(screen.getByRole("textbox", { name: /edit life total/ }))
    fireEvent.click(screen.getByRole("button", { name: "Alice's counters" }))
    fireEvent.click(screen.getByRole("button", { name: "Increase Tymna commander tax" }))
    expect(screen.getByLabelText("Tymna commander tax: 6").textContent).toBe("6")
    fireEvent.click(screen.getByRole("button", { name: "Decrease Thrasios commander tax" }))
    expect(screen.getByLabelText("Thrasios commander tax: 0").textContent).toBe("0")
    fireEvent.click(screen.getByRole("button", { name: "Increase Poison" }))
    expect(screen.getByLabelText("Poison: 4")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Decrease Poison" }))
    expect(screen.getByLabelText("Poison: 3")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Increase Rad" }))
    fireEvent.click(screen.getByRole("button", { name: "Increase Bob · Tymna" }))
    expect(screen.getByLabelText("Bob · Tymna: 1")).toBeTruthy()
    expect(onAdjust.mock.calls).toEqual([
      [{ kind: "casts", commander: "Tymna" }, 1],
      [{ kind: "casts", commander: "Thrasios" }, -1],
      [{ kind: "poison" }, 1],
      [{ kind: "poison" }, -1],
      [{ kind: "rad" }, 1],
      [{ kind: "damage", playerId: 2, commander: "Tymna" }, 1],
    ])
    fireEvent.click(screen.getByRole("button", { name: "Take the monarch" }))
    expect(onTakeMonarch).toHaveBeenCalledOnce()
  })

  it("disables decrement at zero and increment at 999 casts, but allows correction", () => {
    const onAdjust = vi.fn()
    render(
      <SeatCounterControls
        participant={{ ...seat, commander_casts: { Tymna: 999 } }}
        participants={[seat]}
        decks={[deck]}
        local
        monarch={false}
        onAdjust={onAdjust}
        onChangeLife={vi.fn()}
        onTakeMonarch={vi.fn()}
      />,
    )
    fireEvent.click(screen.getByRole("button", { name: "Alice's counters" }))
    expect(screen.getByLabelText("Tymna commander tax: 1998")).toBeTruthy()
    for (const name of [
      "Increase Tymna commander tax",
      "Decrease Thrasios commander tax",
      "Decrease Poison",
    ]) {
      const button = screen.getByRole("button", { name })
      expect(button.hasAttribute("disabled")).toBe(true)
      fireEvent.click(button)
    }
    expect(onAdjust).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("button", { name: "Decrease Tymna commander tax" }))
    expect(onAdjust).toHaveBeenCalledWith({ kind: "casts", commander: "Tymna" }, -1)
  })

  it("opens remote values without mutation buttons, including damage from departed commanders", () => {
    render(
      <SeatCounterControls
        participant={{
          ...seat,
          poison: 5,
          commander_casts: { Tymna: 3, Thrasios: 1 },
          commander_damage: { 99: { Atraxa: 21 } },
        }}
        participants={[seat]}
        decks={[deck]}
        local={false}
        monarch
        onAdjust={vi.fn()}
        onChangeLife={vi.fn()}
        onTakeMonarch={vi.fn()}
      />,
    )
    fireEvent.click(screen.getByRole("button", { name: "Alice's counters" }))
    const panel = screen.getByRole("dialog")
    expect(within(panel).queryAllByRole("button")).toHaveLength(0)
    expect(screen.getByLabelText("Tymna commander tax: 6")).toBeTruthy()
    expect(screen.getByLabelText("Thrasios commander tax: 2")).toBeTruthy()
    expect(screen.getByLabelText("Poison: 5")).toBeTruthy()
    expect(screen.getByLabelText("Player 99 (left) · Atraxa: 21")).toBeTruthy()
  })
})

describe.each([true, false])("seat actions (local=%s)", (local) => {
  it("limits camera/reveal to the owner and exposes pin and eliminate/restore for everyone", async () => {
    const onToggleCamera = vi.fn()
    const onSetEliminated = vi.fn()
    const onTogglePin = vi.fn()
    const onReveal = vi.fn()
    function Seat() {
      const [eliminated, setEliminated] = useState(false)
      return (
        <SeatBar
          participant={{ ...seat, eliminated }}
          local={local}
          decks={[]}
          size="tile"
          pinned={false}
          onChooseDeck={vi.fn()}
          onToggleCamera={onToggleCamera}
          onReveal={onReveal}
          onTogglePin={onTogglePin}
          onSetEliminated={(value) => {
            onSetEliminated(value)
            setEliminated(value)
          }}
        />
      )
    }
    render(<Seat />)
    expect(screen.queryByText("37")).toBeNull()
    if (local) {
      fireEvent.click(screen.getByRole("button", { name: "Turn camera off" }))
      expect(onToggleCamera).toHaveBeenCalledOnce()
      onToggleCamera.mockClear()
    } else {
      expect(screen.queryByRole("button", { name: /camera/ })).toBeNull()
      expect(screen.getByRole("img", { name: "Camera on" })).toBeTruthy()
    }
    const open = async () => {
      const trigger = screen.getByRole("button", { name: "Alice's seat actions" })
      act(() => trigger.focus())
      fireEvent.keyDown(trigger, { key: "ArrowDown" })
      await screen.findByRole("menu")
    }
    await open()
    if (local) {
      fireEvent.click(screen.getByRole("menuitem", { name: "Turn camera off" }))
      expect(onToggleCamera).toHaveBeenCalledOnce()
      await open()
      fireEvent.click(screen.getByRole("menuitem", { name: "Reveal hand…" }))
      expect(onReveal).toHaveBeenCalledOnce()
      await open()
    } else {
      expect(screen.queryByRole("menuitem", { name: /camera|Reveal/ })).toBeNull()
    }
    fireEvent.click(screen.getByRole("menuitem", { name: "Pin as active board" }))
    expect(onTogglePin).toHaveBeenCalledOnce()
    await open()
    if (!local) {
      expect(screen.queryByRole("menuitem", { name: "Eliminate player" })).toBeNull()
      return
    }
    fireEvent.click(screen.getByRole("menuitem", { name: "Eliminate player" }))
    await open()
    fireEvent.click(screen.getByRole("menuitem", { name: "Restore player" }))
    expect(onSetEliminated.mock.calls).toEqual([[true], [false]])
  })
})

it("keeps the commander picker on the owner's name control before selection", () => {
  const onChooseDeck = vi.fn()
  render(
    <CommanderControl
      participant={{ ...seat, deck_id: undefined }}
      decks={[deck]}
      local
      onChooseDeck={onChooseDeck}
    />,
  )
  fireEvent.click(screen.getByRole("button", { name: "Choose Alice's commander" }))
  fireEvent.click(screen.getByText("Partners"))
  expect(onChooseDeck).toHaveBeenCalledWith(7)
})

it("shows remote commander names without a picker", () => {
  render(
    <CommanderControl participant={seat} decks={[deck]} local={false} onChooseDeck={vi.fn()} />,
  )
  expect(screen.queryByRole("button", { name: /commander/ })).toBeNull()
  expect(screen.getByText("Tymna")).toBeTruthy()
  expect(screen.getByText("Thrasios")).toBeTruthy()
  expect(screen.getByTitle("Tymna / Thrasios")).toBeTruthy()
})

it("shows each commander's tax in a badge left of the name, only once a deck is chosen", () => {
  const { rerender } = render(
    <CommanderControl
      participant={{ ...seat, commander_casts: { Tymna: 2 } }}
      decks={[deck]}
      local={false}
      onChooseDeck={vi.fn()}
    />,
  )
  const badge = screen.getByRole("img", { name: "Commander tax: Tymna +4, Thrasios +0" })
  expect(badge.textContent).toBe("+4/+0")
  expect(badge.compareDocumentPosition(screen.getByText("Tymna"))).toBe(
    Node.DOCUMENT_POSITION_FOLLOWING,
  )
  rerender(
    <CommanderControl
      participant={{ ...seat, deck_id: undefined }}
      decks={[deck]}
      local={false}
      onChooseDeck={vi.fn()}
    />,
  )
  expect(screen.queryByRole("img", { name: /Commander tax/ })).toBeNull()
})
