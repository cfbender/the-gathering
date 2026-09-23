import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { commanderNames } from "@/features/decks/decks"
import { cardSnapshot, type SelectedCard } from "@/lib/cards"
import { CommanderActions, commanderPayload } from "./new-commander-dialog"

const commander: SelectedCard = {
  ...cardSnapshot("thrasios", "Thrasios, Triton Hero")!,
  source: "catalog",
  catalog_id: "thrasios",
  color_identity: ["G", "U"],
  printing_id: "thrasios-alternate",
}
const partner: SelectedCard = {
  ...commander,
  id: "tymna",
  catalog_id: "tymna",
  name: "Tymna the Weaver",
  color_identity: ["B", "W"],
  printing_id: "tymna-original",
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function renderActions(playerId = 3) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  client.setQueryData(["session"], { id: 7, role: "member" })
  client.setQueryData(
    ["players"],
    [
      { id: 3, user_id: 7 },
      { id: 4, user_id: 8 },
    ],
  )
  const onChoose = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <CommanderActions playerId={playerId} onChoose={onChoose} />
    </QueryClientProvider>,
  )
  return { client, onChoose }
}

describe("table commander details", () => {
  it("names both commanders consistently without a dangling separator for solo decks", () => {
    expect(commanderNames({ commander_name: commander.name, partner_name: partner.name })).toBe(
      "Thrasios, Triton Hero / Tymna the Weaver",
    )
    expect(commanderNames({ commander_name: commander.name, partner_name: null })).toBe(
      "Thrasios, Triton Hero",
    )
  })

  it("saves independent card identities and printings, trimmed custom name and combined colors", () => {
    expect(commanderPayload("  My partners  ", commander, partner)).toEqual({
      name: "My partners",
      commander_card_id: "thrasios",
      commander_name: "Thrasios, Triton Hero",
      commander_printing_id: "thrasios-alternate",
      partner_card_id: "tymna",
      partner_name: "Tymna the Weaver",
      partner_printing_id: "tymna-original",
      color_identity: "WUBG",
    })
  })

  it("explicitly clears partner identity and printing when removed, and printing when reset", () => {
    expect(
      commanderPayload("Solo", { ...commander, printing_id: null }, null, "WUBG"),
    ).toMatchObject({
      commander_printing_id: null,
      partner_card_id: null,
      partner_name: null,
      partner_printing_id: null,
      color_identity: "UG",
    })
  })

  it("preserves stored colors when legacy card identities cannot be resolved", () => {
    const snapshot = cardSnapshot(null, "Legacy commander")
    expect(commanderPayload("Legacy", snapshot, null, "BR")).toMatchObject({
      commander_card_id: null,
      commander_name: "Legacy commander",
      color_identity: "BR",
    })
  })

  it("does not offer new commanders for another member's seat", () => {
    renderActions(4)
    expect(screen.queryByRole("button", { name: "New commander…" })).toBeNull()
  })

  it("keeps inline validation errors and custom names, then refreshes caches before selecting the saved deck", async () => {
    let rejectName = true
    const fetch = vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/players")
        return new Response(JSON.stringify({ data: [{ id: 3, user_id: 7 }] }))
      if (url === "/api/decks") {
        if (rejectName)
          return new Response(JSON.stringify({ errors: { name: ["has already been taken"] } }), {
            status: 422,
          })
        return new Response(JSON.stringify({ data: { id: 42 } }), { status: 201 })
      }
      if (url.startsWith("/api/cards?")) {
        const card = url.includes("Thrasios") ? commander : partner
        return new Response(JSON.stringify({ data: [{ ...card, printing_id: undefined }] }))
      }
      if (init?.method) throw new Error(`Unexpected write: ${url}`)
      return new Response(JSON.stringify({ data: [] }))
    })
    vi.stubGlobal("fetch", fetch)
    const { client, onChoose } = renderActions()
    const invalidation = vi.spyOn(client, "invalidateQueries")
    fireEvent.click(screen.getByRole("button", { name: "New commander…" }))
    fireEvent.change(screen.getByRole("combobox", { name: "Commander" }), {
      target: { value: "Thrasios" },
    })
    fireEvent.click(await screen.findByRole("option", { name: /Thrasios/ }))
    expect((screen.getByRole("textbox", { name: "Deck name" }) as HTMLInputElement).value).toBe(
      "Thrasios, Triton Hero",
    )
    fireEvent.change(screen.getByRole("textbox", { name: "Deck name" }), {
      target: { value: "My custom pair" },
    })
    fireEvent.change(screen.getByRole("combobox", { name: "Partner (optional)" }), {
      target: { value: "Tymna" },
    })
    fireEvent.click(await screen.findByRole("option", { name: /Tymna/ }))
    expect((screen.getByRole("textbox", { name: "Deck name" }) as HTMLInputElement).value).toBe(
      "My custom pair",
    )
    fireEvent.click(screen.getByRole("button", { name: "Create and select" }))
    expect((await screen.findByRole("alert")).textContent).toContain("name: has already been taken")
    expect(onChoose).not.toHaveBeenCalled()
    expect(invalidation).not.toHaveBeenCalled()
    rejectName = false
    fireEvent.click(screen.getByRole("button", { name: "Create and select" }))
    await waitFor(() => expect(onChoose).toHaveBeenCalledWith(42))
    expect(invalidation).toHaveBeenCalledWith({ queryKey: ["decks"] })
    expect(invalidation).toHaveBeenCalledWith({ queryKey: ["players"] })
    expect(invalidation.mock.invocationCallOrder[0]).toBeLessThan(
      onChoose.mock.invocationCallOrder[0]!,
    )
    expect(screen.queryByRole("dialog")).toBeNull()
    const write = fetch.mock.calls.find(([url]) => url === "/api/decks")
    const body = write?.[1]?.body
    if (typeof body !== "string") throw new Error("Expected a JSON deck payload")
    expect(JSON.parse(body).deck).toMatchObject({
      player_id: 3,
      name: "My custom pair",
      commander_card_id: "thrasios",
      partner_card_id: "tymna",
      color_identity: "WUBG",
    })
    // The table's second card search must include companions, not only Partner cards.
    expect(
      fetch.mock.calls.some(([url]) => url.includes("q=Tymna") && !url.includes("partner=true")),
    ).toBe(true)
  })
})
