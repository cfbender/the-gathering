import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { DeckChooserButton } from "./deck-chooser-button"

function pick(id: number, name: string) {
  return {
    data: {
      deck: {
        id,
        player_id: 1,
        name,
        commander_card_id: null,
        commander_name: `${name} Commander`,
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
      },
      play_count: 1,
      skip_count: 0,
      last_played_at: null,
      reason: null,
    },
  }
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function stubChooser(bodyFor: (method: string, url: string) => unknown) {
  const calls: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      const method = init?.method ?? "GET"
      calls.push(`${method} ${url}`)
      return new Response(JSON.stringify(bodyFor(method, url)), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    }),
  )
  return calls
}

function renderButton() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const onChooseDeck = vi.fn()
  render(
    <QueryClientProvider client={client}>
      <DeckChooserButton onChooseDeck={onChooseDeck} />
    </QueryClientProvider>,
  )
  return { onChooseDeck }
}

describe("table deck chooser", () => {
  it("waits for the popover, skips, then seats the played deck", async () => {
    const calls = stubChooser((method, url) =>
      method === "POST"
        ? { data: { deck_id: 1, outcome: "ok", skip_count: 0 } }
        : url === "/api/deck-chooser?exclude_id=1"
          ? pick(2, "Second deck")
          : pick(1, "First deck"),
    )
    const { onChooseDeck } = renderButton()
    expect(calls).toEqual([])

    fireEvent.click(screen.getByRole("button", { name: "Pick a deck for me" }))
    await screen.findByText("First deck")
    fireEvent.click(screen.getByRole("button", { name: "Skip" }))
    await screen.findByText("Second deck")
    fireEvent.click(screen.getByRole("button", { name: "Play this" }))

    await waitFor(() => expect(onChooseDeck).toHaveBeenCalledWith(2))
    expect(calls).toEqual([
      "GET /api/deck-chooser",
      "POST /api/deck-chooser/1/outcomes",
      "GET /api/deck-chooser?exclude_id=1",
      "POST /api/deck-chooser/2/outcomes",
    ])
    await waitFor(() => expect(screen.queryByText("Second deck")).toBeNull())
  })

  it("explains when there is nothing to pick", async () => {
    stubChooser(() => ({ data: { deck: null, reason: "no_eligible_decks" } }))
    renderButton()

    fireEvent.click(screen.getByRole("button", { name: "Pick a deck for me" }))
    expect(await screen.findByText("None of your decks are included in random picks.")).toBeTruthy()
    expect(screen.queryByRole("button", { name: "Play this" })).toBeNull()
  })
})
