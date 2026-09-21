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
      partner_printing_id: null,
      color_identity: "G",
    })
  })

  it("selects a later-page printing, saves without changing identity or colors, reloads and resets", async () => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    )
    const printing = {
      id: "green-old",
      name: "Green Commander",
      set_code: "old",
      set_name: "Original Set",
      collector_number: "17",
      lang: "ja",
      image_uris: { art_crop: "/old.jpg", normal: "/old-card.jpg" },
    }
    let saved: DeckDetail = { ...deck, color_identity: "WUG" }
    const requests: Record<string, unknown>[] = []
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string, init?: RequestInit) => {
        if (input === "/api/cards/green") return response(card("green", "Green Commander", "G"))
        if (input === "/api/cards/blue") return response(card("blue", "Blue Partner", "U"))
        if (input === "/api/card-printings/green-old") return response(printing)
        if (input.startsWith("/api/card-printings?")) {
          const page = new URL(input, "http://example.test").searchParams.get("page")
          return new Response(
            JSON.stringify({ data: page === "2" ? [printing] : [], has_more: page === "1" }),
            { headers: { "content-type": "application/json" } },
          )
        }
        if (input === "/api/decks/42" && init?.method === "PATCH") {
          const attrs = JSON.parse(init.body as string).deck
          requests.push(attrs)
          saved = { ...saved, ...attrs }
          return response(saved)
        }
        return response([])
      }),
    )
    const mount = () =>
      render(
        <QueryClientProvider
          client={
            new QueryClient({
              defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
            })
          }
        >
          <DeckEditForm deck={saved} />
        </QueryClientProvider>,
      )
    const first = mount()
    fireEvent.click(
      await screen.findByRole("button", { name: "Commander printing: Catalog default" }),
    )
    fireEvent.click(await screen.findByRole("button", { name: "More printings" }))
    fireEvent.click(await screen.findByRole("button", { name: "Original Set (OLD) #17 · JA" }))
    expect(screen.queryByRole("dialog")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Save deck" }))
    await screen.findByText("Deck details saved.")
    expect(requests[0]).toMatchObject({
      commander_card_id: "green",
      commander_name: "Green Commander",
      commander_printing_id: "green-old",
      partner_card_id: "blue",
      color_identity: "WUG",
    })
    first.unmount()
    mount()
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Commander printing: Original Set (OLD) #17 · JA",
      }),
    )
    fireEvent.click(screen.getByRole("button", { name: "Use catalog default" }))
    fireEvent.click(screen.getByRole("button", { name: "Save deck" }))
    await screen.findByText("Deck details saved.")
    expect(requests[1]).toMatchObject({
      commander_card_id: "green",
      commander_printing_id: null,
      color_identity: "WUG",
    })
  })

  it("keeps a saved printing when options fail and removes it with the partner", async () => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    )
    let patch: Record<string, unknown> | undefined
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string, init?: RequestInit) => {
        if (input === "/api/cards/green") return response(card("green", "Green Commander", "G"))
        if (input === "/api/cards/blue") return response(card("blue", "Blue Partner", "U"))
        if (input === "/api/card-printings/blue-old")
          return response({
            id: "blue-old",
            set_name: "Partner Set",
            set_code: "ptr",
            collector_number: "3",
            lang: "en",
            image_uris: {},
          })
        if (input.startsWith("/api/card-printings?"))
          return new Response(JSON.stringify({ errors: { detail: "Bad Gateway" } }), {
            status: 502,
          })
        if (init?.method === "PATCH") {
          patch = JSON.parse(init.body as string).deck
          return response(deck)
        }
        return response([])
      }),
    )
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <DeckEditForm deck={{ ...deck, partner_printing_id: "blue-old" }} />
      </QueryClientProvider>,
    )
    fireEvent.click(
      await screen.findByRole("button", { name: "Partner printing: Partner Set (PTR) #3" }),
    )
    expect((await screen.findByRole("alert")).textContent).toContain("Your selection is unchanged")
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }))
    fireEvent.click(screen.getByRole("button", { name: "Save deck" }))
    await screen.findByText("Deck details saved.")
    expect(patch?.partner_printing_id).toBe("blue-old")
    const input = screen.getByRole("combobox", { name: "Partner (optional)" })
    fireEvent.click(within(input.parentElement!).getByRole("button", { name: "Clear card" }))
    expect(screen.queryByRole("button", { name: /Partner printing:/ })).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "Save deck" }))
    await waitFor(() => expect(patch?.partner_printing_id).toBeNull())
    expect(patch?.partner_card_id).toBeNull()
  })
})

function response(data: unknown) {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { "content-type": "application/json" },
  })
}
