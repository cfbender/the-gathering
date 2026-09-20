import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { CardSummary } from "@/lib/cards"
import { CardSearch } from "./card-search"

const cards: CardSummary[] = [
  {
    id: "atraxa",
    oracle_id: "oracle-atraxa",
    name: "Atraxa, Praetors' Voice",
    mana_cost: "{G}{W}{U}{B}",
    type_line: "Legendary Creature — Phyrexian Angel Horror",
    color_identity: ["W", "U", "B", "G"],
    image_uris: {},
    can_be_commander: true,
    commander_pairing: null,
  },
  {
    id: "other",
    oracle_id: "oracle-other",
    name: "Atraxa's Skitterfang",
    mana_cost: "{3}",
    type_line: "Artifact Creature — Phyrexian Insect",
    color_identity: [],
    image_uris: {},
    can_be_commander: false,
    commander_pairing: null,
  },
]

function Harness() {
  const [value, setValue] = useState<CardSummary | null>(null)
  return <CardSearch label="Commander" value={value} onChange={setValue} mode="commander" />
}

function SelectedHarness() {
  const [value, setValue] = useState<CardSummary | null>(cards[0] ?? null)
  return <CardSearch label="Commander" value={value} onChange={setValue} mode="commander" />
}

function renderSearch(component = <Harness />) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(<QueryClientProvider client={queryClient}>{component}</QueryClientProvider>)
}

describe("CardSearch", () => {
  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it("navigates results with the keyboard, selects, and clears a card", async () => {
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: cards }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    )
    vi.stubGlobal("fetch", fetch)
    renderSearch()

    const combobox = screen.getByRole("combobox", { name: "Commander" })
    fireEvent.change(combobox, { target: { value: "Atraxa" } })

    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(2))
    expect(fetch).toHaveBeenCalledWith(
      "/api/cards?q=Atraxa&limit=20&commander=true",
      expect.objectContaining({ credentials: "same-origin" }),
    )

    fireEvent.keyDown(combobox, { key: "ArrowDown" })
    expect(screen.getAllByRole("option").at(0)?.getAttribute("aria-selected")).toBe("true")
    fireEvent.keyDown(combobox, { key: "Enter" })

    expect(screen.getByText("Legendary Creature — Phyrexian Angel Horror")).toBeTruthy()
    expect(combobox.getAttribute("aria-expanded")).toBe("false")

    fireEvent.click(screen.getByRole("button", { name: "Clear card" }))
    expect((combobox as HTMLInputElement).value).toBe("")
  })

  it("keeps the first keystroke when editing a selected card", () => {
    renderSearch(<SelectedHarness />)

    const combobox = screen.getByRole("combobox", { name: "Commander" })
    fireEvent.change(combobox, { target: { value: "Atraxa!" } })

    expect((combobox as HTMLInputElement).value).toBe("Atraxa!")
    expect(screen.queryByText("Legendary Creature — Phyrexian Angel Horror")).toBeNull()
  })

  it("searches partner-eligible cards in partner mode instead of primary commanders", async () => {
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ data: [] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    )
    vi.stubGlobal("fetch", fetch)
    renderSearch(<CardSearch label="Partner" value={null} onChange={() => {}} mode="partner" />)

    fireEvent.change(screen.getByRole("combobox", { name: "Partner" }), {
      target: { value: "Background" },
    })

    await waitFor(() => expect(fetch).toHaveBeenCalled())
    expect(fetch).toHaveBeenCalledWith(
      "/api/cards?q=Background&limit=20&partner=true",
      expect.anything(),
    )
  })
})
