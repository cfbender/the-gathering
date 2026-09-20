import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { CardSummary } from "@/lib/cards"
import type { DeckDetail } from "@/features/decks/decks"
import { DeckEditForm } from "./decks.$deckId"

const card = (id: string, name: string, color: string): CardSummary => ({
  id,
  oracle_id: `oracle-${id}`,
  name,
  mana_cost: null,
  type_line: "Legendary Creature",
  color_identity: [color],
  image_uris: {},
  can_be_commander: true,
  commander_pairing: "partner",
})

const deck: DeckDetail = {
  id: 42,
  player_id: 7,
  name: "Catalog partners",
  commander_card_id: "green",
  commander_name: "Green Commander",
  commander_art_crop_url: null,
  partner_card_id: "blue",
  partner_name: "Blue Partner",
  partner_art_crop_url: null,
  color_identity: "UG",
  decklist_url: null,
  decklist_source: null,
  archived_at: null,
  skip_count: 0,
  included_for_play: true,
  player: null,
  games_played: 0,
  wins: 0,
  recent_games: [],
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("deck editor", () => {
  it("persists explicit null partner fields and the remaining commander's identity", async () => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    )
    let patchRequest: RequestInit | undefined
    const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      if (url === "/api/cards/green") return response(card("green", "Green Commander", "G"))
      if (url === "/api/cards/blue") return response(card("blue", "Blue Partner", "U"))
      if (url === "/api/decks/42" && init?.method === "PATCH") {
        patchRequest = init
        return response({ ...deck, partner_card_id: null, partner_name: null, color_identity: "G" })
      }
      return response([])
    })
    vi.stubGlobal("fetch", fetch)
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    })

    render(
      <QueryClientProvider client={queryClient}>
        <DeckEditForm deck={deck} />
      </QueryClientProvider>,
    )

    const partnerInput = await screen.findByRole("combobox", { name: "Partner (optional)" })
    fireEvent.click(within(partnerInput.parentElement!).getByRole("button", { name: "Clear card" }))
    fireEvent.click(screen.getByRole("button", { name: "Save deck" }))

    await waitFor(() => expect(patchRequest).toBeDefined())
    expect(typeof patchRequest?.body).toBe("string")
    const payload = JSON.parse(patchRequest!.body as string)
    expect(payload.deck).toMatchObject({
      partner_card_id: null,
      partner_name: null,
      color_identity: "G",
    })
  })
})

function response(data: unknown) {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { "content-type": "application/json" },
  })
}
