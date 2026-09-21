import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { RetireDeckCard } from "./retire-deck"
import type { DeckDetail } from "./decks"

const deck: DeckDetail = {
  id: 42,
  player_id: 7,
  name: "Krenko",
  commander_card_id: null,
  commander_name: "Krenko, Mob Boss",
  commander_art_crop_url: null,
  partner_card_id: null,
  partner_name: null,
  partner_art_crop_url: null,
  color_identity: "R",
  decklist_url: null,
  decklist_source: null,
  archived_at: null,
  skip_count: 0,
  included_for_play: true,
  player: null,
  games_played: 3,
  wins: 1,
  recent_games: [],
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function renderCard(initial: DeckDetail) {
  const patches: unknown[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      if (url === "/api/decks/42" && init?.method === "PATCH") {
        const payload = JSON.parse(init.body as string) as { deck: { archived_at: string | null } }
        patches.push(payload.deck)
        return response({ ...initial, archived_at: payload.deck.archived_at })
      }
      throw new Error(`unexpected request ${url}`)
    }),
  )
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  queryClient.setQueryData(["decks", "42"], initial)
  render(
    <QueryClientProvider client={queryClient}>
      <RetireDeckCard deck={initial} />
    </QueryClientProvider>,
  )
  return { patches, queryClient }
}

describe("RetireDeckCard", () => {
  it("retires an active deck by stamping archived_at with the current time", async () => {
    vi.useFakeTimers({ now: new Date("2026-09-21T03:00:00.000Z"), toFake: ["Date"] })
    const { patches, queryClient } = renderCard(deck)

    fireEvent.click(screen.getByRole("button", { name: "Retire deck" }))

    await waitFor(() => expect(patches).toHaveLength(1))
    expect(patches[0]).toEqual({ archived_at: "2026-09-21T03:00:00.000Z" })
    await waitFor(() =>
      expect(queryClient.getQueryData<DeckDetail>(["decks", "42"])?.archived_at).toBe(
        "2026-09-21T03:00:00.000Z",
      ),
    )
    vi.useRealTimers()
  })

  it("brings a retired deck back with an explicit null", async () => {
    const retired = { ...deck, archived_at: "2026-01-01T00:00:00Z" }
    const { patches } = renderCard(retired)

    expect(screen.getByRole("heading", { name: "Retired deck" })).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Bring back" }))

    await waitFor(() => expect(patches).toHaveLength(1))
    expect(patches[0]).toEqual({ archived_at: null })
  })
})

function response(data: unknown) {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { "content-type": "application/json" },
  })
}
