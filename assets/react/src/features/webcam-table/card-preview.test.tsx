import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { CardPreview } from "./card-preview"

afterEach(cleanup)

function preview() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  client.setQueryData(["card-printings", "bolt", "details"], {
    name: "Lightning Bolt",
    image_uris: {},
    mana_cost: "{R}",
    type_line: "Instant",
    oracle_text: "Lightning Bolt deals 3 damage to any target.",
    power: null,
    toughness: null,
    loyalty: null,
    set_code: "lea",
    collector_number: "161",
  })
  client.setQueryData(["card-printings", "bolt", "rulings"], [])
  const onClose = vi.fn()
  const onWrongCard = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <CardPreview
        card={{ id: "bolt", name: "Lightning Bolt", set: "lea" }}
        onClose={onClose}
        onWrongCard={onWrongCard}
        onRemove={vi.fn()}
      />
    </QueryClientProvider>,
  )
  return { onClose, onWrongCard }
}

it("keeps content and toolbar clicks inside, while layout gaps and backdrop close", () => {
  const { onClose, onWrongCard } = preview()
  fireEvent.click(screen.getByLabelText("Rules text"))
  fireEvent.click(screen.getByRole("button", { name: "Wrong card?" }))
  expect(onWrongCard).toHaveBeenCalledOnce()
  expect(onClose).not.toHaveBeenCalled()
  // This layout wrapper covers the empty area below the shorter rules panel.
  const rules = screen.getByLabelText("Rules text")
  fireEvent.click(rules.parentElement!)
  expect(onClose).toHaveBeenCalledOnce()
  fireEvent.click(screen.getByRole("dialog", { name: "Lightning Bolt details" }))
  expect(onClose).toHaveBeenCalledTimes(2)
})

it("opens rulings with right-click and closes only rulings on Escape", () => {
  const { onClose } = preview()
  fireEvent.contextMenu(screen.getByLabelText("Rules text"))
  expect(screen.getByRole("dialog", { name: "Lightning Bolt · Rulings" })).toBeTruthy()
  expect(screen.getByText("No rulings published on Scryfall for this card.")).toBeTruthy()
  fireEvent.keyDown(document.activeElement!, { key: "Escape" })
  expect(screen.queryByRole("dialog", { name: "Lightning Bolt · Rulings" })).toBeNull()
  expect(onClose).not.toHaveBeenCalled()
  const handledEscape = new KeyboardEvent("keydown", { key: "Escape", cancelable: true })
  handledEscape.preventDefault()
  fireEvent(window, handledEscape)
  expect(onClose).not.toHaveBeenCalled()
  fireEvent.keyDown(window, { key: "Escape" })
  expect(onClose).toHaveBeenCalledOnce()
})
