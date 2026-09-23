import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { DeckChooserPage } from "@/features/decks/deck-chooser-page"
import type { User } from "@/lib/auth"

const user: User = {
  id: 7,
  username: "cody",
  display_name: "Cody",
  discord_id: null,
  avatar_url: null,
  moxfield_username: null,
  archidekt_username: null,
  manavault_url: "https://vault.example.com",
  has_manavault_api_key: false,
  has_password: true,
  role: "admin",
  disabled: false,
  inserted_at: "2026-09-20T12:00:00Z",
}

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
      play_count: 2,
      skip_count: 0,
      last_played_at: null,
      reason: null,
    },
  }
}

function renderPage(currentUser = user) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity },
      mutations: { retry: false },
    },
  })
  queryClient.setQueryData(["session"], currentUser)
  return render(
    <QueryClientProvider client={queryClient}>
      <DeckChooserPage />
    </QueryClientProvider>,
  )
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("deck chooser", () => {
  it.each([true, false, undefined])(
    "labels a partner Game Changer only when flagged: %s",
    async (flag) => {
      const body = pick(1, "Partner deck")
      const deck = {
        ...body.data.deck,
        commander_name: "Kraum, Ludevic's Opus",
        commander_game_changer: false,
        partner_name: "Thrasios, Triton Hero",
        partner_game_changer: flag,
      }
      vi.stubGlobal(
        "fetch",
        vi.fn().mockResolvedValue(
          new Response(JSON.stringify({ data: { ...body.data, deck } }), {
            headers: { "content-type": "application/json" },
          }),
        ),
      )
      renderPage()
      await screen.findByText("Thrasios, Triton Hero")
      expect(screen.queryAllByText("Game Changer")).toHaveLength(flag ? 1 : 0)
    },
  )

  it("hides hosted-deck sync until a deck host is configured", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ data: { deck: null, reason: "no_eligible_decks" } }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    )

    renderPage()
    await screen.findByText("No decks available to pick")
    expect(screen.queryByRole("button", { name: "Sync hosted decks" })).toBeNull()

    cleanup()
    renderPage({ ...user, has_manavault_api_key: true })
    expect(await screen.findByRole("button", { name: "Sync hosted decks" })).toBeTruthy()
  })

  it("records a skip before requesting a different candidate", async () => {
    const calls: string[] = []
    const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      calls.push(`${init?.method ?? "GET"} ${url}`)

      const body =
        url === "/api/deck-chooser?exclude_id=1"
          ? pick(2, "Second deck")
          : url === "/api/deck-chooser"
            ? pick(1, "First deck")
            : { data: { deck_id: 1, outcome: "skipped", skip_count: 1 } }

      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    })
    vi.stubGlobal("fetch", fetch)
    renderPage()

    await screen.findByRole("heading", { name: "First deck" })
    fireEvent.click(screen.getByRole("button", { name: "Skip" }))
    await screen.findByRole("heading", { name: "Second deck" })

    expect(calls).toEqual([
      "GET /api/deck-chooser",
      "POST /api/deck-chooser/1/outcomes",
      "GET /api/deck-chooser?exclude_id=1",
    ])
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(3))
  })
})
