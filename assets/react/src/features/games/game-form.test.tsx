import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { Game, PlayerSummary } from "@/features/games/games"
import type { DeckSummary } from "@/features/decks/decks"
import { GameForm } from "./game-form"

const navigate = vi.hoisted(() => vi.fn())

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}))

const player = (id: number, name: string, archived = false): PlayerSummary => ({
  id,
  name,
  avatar_url: null,
  user_id: null,
  archived_at: archived ? "2026-01-01T00:00:00Z" : null,
})

const alice = player(1, "Alice")
const bob = player(2, "Bob", true)
const cara = player(3, "Cara")
const archivedDeck: DeckSummary = {
  id: 22,
  player_id: bob.id,
  name: "Retired artifacts",
  commander_card_id: "urza",
  commander_name: "Urza, Lord High Artificer",
  commander_art_crop_url: null,
  partner_card_id: null,
  partner_name: null,
  partner_art_crop_url: null,
  color_identity: "U",
  decklist_url: null,
  decklist_source: null,
  archived_at: "2026-01-01T00:00:00Z",
  skip_count: 0,
  included_for_play: true,
  player: bob,
}

function gameFixture(): Game {
  return {
    id: 44,
    played_at: "2026-09-19T18:00:00Z",
    duration_minutes: 75,
    turns: 9,
    notes: "Original notes",
    source: "manual",
    external_id: null,
    created_by_user_id: 1,
    seats: [alice, bob, cara].map((seatPlayer, index) => ({
      id: 101 + index,
      player_id: seatPlayer.id,
      deck_id: index === 1 ? archivedDeck.id : null,
      seat: index + 1,
      result: index === 1 ? "win" : "loss",
      mvp_card_id: index === 0 ? "rhystic-study" : null,
      mvp_card_name: index === 0 ? "Rhystic Study" : null,
      mvp_art_crop_url: null,
      notes: null,
      player: seatPlayer,
      deck: index === 1 ? archivedDeck : null,
    })),
  }
}

function response(data: unknown) {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { "content-type": "application/json" },
  })
}

function renderGame(game = gameFixture()) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity },
      mutations: { retry: false },
    },
  })
  queryClient.setQueryData(["players"], [alice, cara])
  queryClient.setQueryData(["decks", {}], [])

  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    if (url === "/api/players") return response([alice, cara])
    if (url === "/api/decks") return response([])
    if (url === "/api/games/44" && init?.method === "PATCH") return response(game)
    throw new Error(`Unexpected request: ${init?.method ?? "GET"} ${url}`)
  })
  vi.stubGlobal("fetch", fetch)

  const Wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  const view = render(<GameForm game={game} />, { wrapper: Wrapper })
  return { ...view, fetch, game, queryClient }
}

async function submitPayload(fetch: ReturnType<typeof vi.fn>) {
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }))
  await waitFor(() => expect(navigate).toHaveBeenCalled())
  const call = fetch.mock.calls.find(
    ([input, init]) => input === "/api/games/44" && init?.method === "PATCH",
  )
  expect(call).toBeTruthy()
  return JSON.parse(String(call?.[1]?.body)).game as {
    notes: string | null
    turns: number | null
    seats: Array<{
      id: number
      player_id: number
      deck_id: number | null
      seat: number
      result: "win" | "loss" | "draw"
      mvp_card_id: string | null
      mvp_card_name: string | null
    }>
  }
}

afterEach(() => {
  cleanup()
  navigate.mockReset()
  vi.unstubAllGlobals()
})

describe("GameForm submissions", () => {
  it("keeps the winner attached to the same seat after moving and removing seats", async () => {
    const { fetch } = renderGame()

    const bobSeat = screen.getByDisplayValue("Bob").closest("article")
    expect(bobSeat).toBeTruthy()
    fireEvent.click(within(bobSeat as HTMLElement).getByRole("button", { name: "Move seat down" }))
    const aliceSeat = screen.getByDisplayValue("Alice").closest("article")
    fireEvent.click(within(aliceSeat as HTMLElement).getByRole("button", { name: "Remove seat" }))

    const payload = await submitPayload(fetch)
    expect(
      payload.seats.map(({ player_id, seat, result }) => ({ player_id, seat, result })),
    ).toEqual([
      { player_id: 3, seat: 1, result: "loss" },
      { player_id: 2, seat: 2, result: "win" },
    ])
  })

  it("submits an archived player's persisted ID without trying to recreate the player", async () => {
    const { fetch } = renderGame()

    const payload = await submitPayload(fetch)

    expect(payload.seats.find((seat) => seat.id === 102)).toMatchObject({
      player_id: 2,
      deck_id: 22,
    })
    expect(
      fetch.mock.calls.some(([input, init]) => input === "/api/players" && init?.method === "POST"),
    ).toBe(false)
    expect(
      fetch.mock.calls.some(([input, init]) => input === "/api/decks" && init?.method === "POST"),
    ).toBe(false)
  })

  it("does not replace a dirty draft when the same game refreshes", async () => {
    const { fetch, game, rerender } = renderGame()
    fireEvent.change(screen.getByRole("textbox", { name: "Notes" }), {
      target: { value: "Unsaved local notes" },
    })

    rerender(<GameForm game={{ ...game, notes: "Refetched server notes", turns: 99 }} />)

    expect((screen.getByRole("textbox", { name: "Notes" }) as HTMLTextAreaElement).value).toBe(
      "Unsaved local notes",
    )
    const payload = await submitPayload(fetch)
    expect(payload.notes).toBe("Unsaved local notes")
    expect(payload.turns).toBe(9)
  })

  it("sends null for both MVP fields when a persisted selection is cleared", async () => {
    const { fetch } = renderGame()
    fireEvent.click(screen.getByRole("button", { name: "Clear card" }))

    const payload = await submitPayload(fetch)

    expect(payload.seats.find((seat) => seat.id === 101)).toMatchObject({
      mvp_card_id: null,
      mvp_card_name: null,
    })
  })

  it("invalidates stats after saving", async () => {
    const { fetch, queryClient } = renderGame()
    queryClient.setQueryData(["stats", "overview"], { games_count: 1 })

    await submitPayload(fetch)

    expect(queryClient.getQueryState(["stats", "overview"])?.isInvalidated).toBe(true)
  })
})
