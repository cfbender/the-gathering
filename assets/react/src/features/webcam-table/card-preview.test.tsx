import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { CardPreview } from "./card-preview"
import { printingPrices, type PrintingDetails } from "./card-details"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const printings = [
  {
    id: "new",
    name: "Lightning Bolt",
    set_code: "m11",
    set_name: "Magic 2011",
    collector_number: "146",
    lang: "en",
    image_uris: { normal: "https://img.example/new.jpg" },
  },
  {
    id: "bolt",
    name: "Lightning Bolt",
    set_code: "lea",
    set_name: "Limited Edition Alpha",
    collector_number: "161",
    lang: "en",
    image_uris: { normal: "https://img.example/bolt.jpg" },
  },
]

function preview(id = "bolt", fetchList = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  if (!fetchList) client.setQueryData(["card-printings", "preview", "Lightning Bolt"], printings)
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
    prices: { usd: "0.25", usd_foil: "1.10", usd_etched: null },
  })
  client.setQueryData(["card-printings", "new", "details"], {
    ...client.getQueryData<PrintingDetails>(["card-printings", "bolt", "details"]),
    set_code: "m11",
    set_name: "Magic 2011",
    collector_number: "146",
    prices: { usd: null, usd_foil: null, usd_etched: null },
  })
  client.setQueryData(["card-printings", "outside-1", "details"], {
    ...client.getQueryData<PrintingDetails>(["card-printings", "bolt", "details"]),
    set_code: "fin",
    collector_number: "99",
  })
  client.setQueryData(["card-printings", "bolt", "rulings"], [])
  const onClose = vi.fn()
  const onWrongCard = vi.fn()
  const onRemove = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <CardPreview
        card={{ id, name: "Lightning Bolt", set: "lea" }}
        onClose={onClose}
        onWrongCard={onWrongCard}
        onRemove={onRemove}
      />
    </QueryClientProvider>,
  )
  return { onClose, onWrongCard, onRemove }
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

it("starts on the clicked printing, wraps in API order, and changes prices and caption without editing", () => {
  const { onRemove, onWrongCard } = preview()
  expect(screen.getByText("2 / 2")).toBeTruthy()
  expect(screen.getByText("$0.25 · Foil $1.10")).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Next printing" }))
  expect(screen.getByText("1 / 2")).toBeTruthy()
  expect(screen.getByText("Magic 2011 · #146")).toBeTruthy()
  expect(screen.getByLabelText("Prices in USD").textContent).toBe("—")
  fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" })
  expect(screen.getByText("2 / 2")).toBeTruthy()
  expect(screen.getByText("LEA · #161")).toBeTruthy()
  fireEvent.keyDown(document.activeElement!, { key: "ArrowRight" })
  expect(screen.getByText("1 / 2")).toBeTruthy()
  expect(onRemove).not.toHaveBeenCalled()
  expect(onWrongCard).not.toHaveBeenCalled()
})

it("ignores arrows outside the preview, in inputs, and while rulings are open", () => {
  preview()
  const dialog = screen.getByRole("dialog", { name: "Lightning Bolt details" })
  const input = document.createElement("input")
  dialog.append(input)
  input.focus()
  fireEvent.keyDown(input, { key: "ArrowRight" })
  expect(screen.getByText("2 / 2")).toBeTruthy()
  document.body.append(input)
  input.focus()
  fireEvent.keyDown(input, { key: "ArrowLeft" })
  expect(screen.getByText("2 / 2")).toBeTruthy()
  input.remove()
  dialog.focus()
  for (const event of [
    { key: "." },
    { key: "," },
    { key: "ArrowLeft", ctrlKey: true },
    { key: "ArrowRight", repeat: true },
  ]) {
    fireEvent.keyDown(dialog, event)
  }
  expect(screen.getByText("2 / 2")).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Rulings" }))
  fireEvent.keyDown(document.activeElement!, { key: "ArrowRight" })
  expect(screen.getByText("2 / 2")).toBeTruthy()
  // Even a dispatched key on the underlying preview must not bypass the rulings guard.
  fireEvent.keyDown(dialog, { key: "ArrowLeft" })
  expect(screen.getByText("2 / 2")).toBeTruthy()
})

it("loads every page and prepends an absent opaque printing id", async () => {
  const fetch = vi.spyOn(globalThis, "fetch").mockImplementation(async (url) => {
    const page = new URL(
      url instanceof Request ? url.url : url,
      "https://example.test",
    ).searchParams.get("page")
    return new Response(
      JSON.stringify({ data: [printings[page === "1" ? 0 : 1]], has_more: page === "1" }),
      { status: 200 },
    )
  })
  preview("outside-1", true)
  await screen.findByText("1 / 3")
  expect(fetch.mock.calls.map(([url]) => url)).toEqual([
    "/api/card-printings?name=Lightning+Bolt&page=1",
    "/api/card-printings?name=Lightning+Bolt&page=2",
  ])
  fireEvent.click(screen.getByRole("button", { name: "Next printing" }))
  expect(screen.getByText("Magic 2011 · #146")).toBeTruthy()
  expect(screen.getByText("2 / 3")).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Previous printing" }))
  expect(screen.getByText("FIN · #99")).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Previous printing" }))
  expect(screen.getByText("3 / 3")).toBeTruthy()
})

it("retains the clicked card when the list fails and offers retry", async () => {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}", { status: 503 }))
  preview("bolt", true)
  await screen.findByText(/Printings unavailable/)
  expect(screen.getByText("LEA · #161")).toBeTruthy()
  expect(screen.getByRole("button", { name: "Next printing" }).hasAttribute("disabled")).toBe(true)
  vi.mocked(fetch).mockResolvedValue(
    new Response(JSON.stringify({ data: printings, has_more: false })),
  )
  fireEvent.click(screen.getByRole("button", { name: "Retry" }))
  await waitFor(() => expect(screen.getByText("2 / 2")).toBeTruthy())
})

it("formats foil-only and etched prices without inventing a nonfoil price", () => {
  expect(printingPrices({ usd: null, usd_foil: "1.10", usd_etched: "2.20" })).toBe(
    "Foil $1.10 · Etched $2.20",
  )
  expect(printingPrices({ usd: "0.00", usd_foil: null, usd_etched: null })).toBe("$0.00")
  // A details payload cached before prices existed must not crash the preview.
  expect(printingPrices(undefined)).toBe("—")
})
