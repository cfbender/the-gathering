import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { Game, PlayerSummary } from "@/features/games/games"
import type { DeckSummary } from "@/features/decks/decks"
import type { DiscordResultDraft } from "./use-game-draft"
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
    format: "commander",
    played_at: "2026-09-19T18:00:00Z",
    duration_minutes: 75,
    turns: 9,
    win_condition: "combat_damage",
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
      kills: index === 0 ? 0 : null,
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

function renderDiscordGame(winner: string | null = "discord-bob") {
  const draft: DiscordResultDraft = {
    id: "draft-uuid",
    external_id: "spellbot-123",
    played_at: "2026-09-19T18:00:00Z",
    duration_minutes: 83,
    winner_discord_id: winner,
    seats: [
      { discord_id: "discord-alice", player_id: 1, player_name: "Alice" },
      { discord_id: "discord-bob", player_id: 2, player_name: "Bob" },
      { discord_id: "discord-cara", player_id: null, player_name: "Cara" },
    ],
  }
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity }, mutations: { retry: false } },
  })
  queryClient.setQueryData(["players"], [alice, bob, cara])
  queryClient.setQueryData(["decks", {}], [archivedDeck])
  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
    if (url === "/api/discord/result-drafts/draft-uuid" && init?.method === "POST") {
      return response(gameFixture())
    }
    throw new Error(`Unexpected request: ${init?.method ?? "GET"} ${url}`)
  })
  vi.stubGlobal("fetch", fetch)
  const Wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  return { ...render(<GameForm discordDraft={draft} />, { wrapper: Wrapper }), fetch }
}

async function submitPayload(fetch: ReturnType<typeof vi.fn>) {
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }))
  await waitFor(() => expect(navigate).toHaveBeenCalled())
  const call = fetch.mock.calls.find(
    ([input, init]) => input === "/api/games/44" && init?.method === "PATCH",
  )
  expect(call).toBeTruthy()
  return JSON.parse(String(call?.[1]?.body)).game as {
    format: Game["format"]
    notes: string | null
    win_condition: string | null
    turns: number | null
    seats: Array<{
      id: number
      player_id: number
      deck_id: number | null
      seat: number
      result: "win" | "loss" | "draw"
      kills: number | null
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
  it("defaults to Commander and records both adjacent teammates when selecting 2HG", async () => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    })
    const { fetch } = renderGame()
    const select = screen.getByRole("combobox", { name: "Format" })
    expect(select.textContent).toContain("Commander")
    fireEvent.click(select)
    fireEvent.click(await screen.findByRole("option", { name: "2HG" }))
    const payload = await submitPayload(fetch)
    expect(payload.format).toBe("two_headed_giant")
    expect(payload.seats.map((seat) => seat.result)).toEqual(["win", "win", "loss"])
  })

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

  it("shows nested seat errors as text instead of crashing the page", async () => {
    const { fetch } = renderGame()
    fetch.mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
      if (input === "/api/games/44" && init?.method === "PATCH") {
        return new Response(
          JSON.stringify({ errors: { seats: [{}, { seat: ["has already been taken"] }] } }),
          { status: 422, headers: { "content-type": "application/json" } },
        )
      }
      return response([])
    })

    fireEvent.click(screen.getByRole("button", { name: "Save changes" }))
    expect(await screen.findByText("Seat 2: seat has already been taken")).toBeTruthy()
    expect(screen.getByRole("alert").textContent).toContain("Could not save the game")
    expect(navigate).not.toHaveBeenCalled()
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

  it("edits and submits the optional win condition", async () => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    })
    const { fetch } = renderGame()
    const select = screen.getByRole("combobox", { name: "Win condition" })
    expect(select.textContent).toContain("Combat Damage")

    fireEvent.click(select)
    fireEvent.click(await screen.findByRole("option", { name: "Infinite Combo" }))
    const payload = await submitPayload(fetch)

    expect(payload.win_condition).toBe("infinite_combo")
  })

  it("submits a blank win condition as null", async () => {
    const { fetch } = renderGame({ ...gameFixture(), win_condition: null })

    const payload = await submitPayload(fetch)

    expect(payload.win_condition).toBeNull()
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

  it("preserves explicit zero kills and submits a blank count as unknown", async () => {
    const { fetch } = renderGame()
    const killInputs = screen.getAllByRole("spinbutton", { name: "Kills (optional)" })

    expect((killInputs[0] as HTMLInputElement).value).toBe("0")
    expect((killInputs[1] as HTMLInputElement).value).toBe("")

    const payload = await submitPayload(fetch)
    expect(payload.seats.map((seat) => seat.kills)).toEqual([0, null, null])
  })

  it("clears a recorded zero to unknown when the input is emptied", async () => {
    const { fetch } = renderGame()
    fireEvent.change(screen.getAllByRole("spinbutton", { name: "Kills (optional)" })[0]!, {
      target: { value: "" },
    })
    const payload = await submitPayload(fetch)
    expect(payload.seats[0]?.kills).toBeNull()
  })

  it.each([
    ["-1", "rangeUnderflow"],
    ["1.5", "stepMismatch"],
  ])("rejects an invalid kill count of %s", (value, validityFlag) => {
    const { fetch } = renderGame()
    const input = screen.getAllByRole("spinbutton", { name: "Kills (optional)" })[0]
    expect(input).toBeTruthy()

    fireEvent.change(input as HTMLInputElement, { target: { value } })
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }))

    expect((input as HTMLInputElement).validity[validityFlag as "rangeUnderflow"]).toBe(true)
    expect(
      fetch.mock.calls.some(
        ([request, init]) => request === "/api/games/44" && init?.method === "PATCH",
      ),
    ).toBe(false)
  })

  it("invalidates stats after saving", async () => {
    const { fetch, queryClient } = renderGame()
    queryClient.setQueryData(["stats", "overview"], { games_count: 1 })

    await submitPayload(fetch)

    expect(queryClient.getQueryState(["stats", "overview"])?.isInvalidated).toBe(true)
  })
})

describe("GameForm Discord handoff", () => {
  it("does not assume a winner or draw when /log omitted the winner", () => {
    const { fetch } = renderDiscordGame(null)
    expect(screen.getByRole("button", { name: "Log game" }).hasAttribute("disabled")).toBe(true)
    expect(
      screen.getAllByRole("radio").every((radio) => !(radio as HTMLInputElement).checked),
    ).toBe(true)
    expect(
      (screen.getByRole("checkbox", { name: "Game ended in a draw" }) as HTMLInputElement).checked,
    ).toBe(false)
    expect(fetch).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole("radio", { name: "Cara won" }))
    expect(screen.getByRole("button", { name: "Log game" }).hasAttribute("disabled")).toBe(false)
  })

  it("prefills the roster and metadata, keeps identities fixed, and submits one atomic request", async () => {
    const { fetch } = renderDiscordGame()

    expect(screen.getByText(/SpellBot game spellbot-123/)).toBeTruthy()
    expect((screen.getByRole("spinbutton", { name: "Minutes" }) as HTMLInputElement).value).toBe(
      "83",
    )
    expect(screen.getAllByRole("combobox", { name: "Player" })).toHaveLength(3)
    expect(
      screen
        .getAllByRole("combobox", { name: "Player" })
        .every((input) => input.hasAttribute("disabled")),
    ).toBe(true)
    expect(screen.getByRole("button", { name: "Add seat" }).hasAttribute("disabled")).toBe(true)
    expect(
      screen
        .getAllByRole("button", { name: "Remove seat" })
        .every((button) => button.hasAttribute("disabled")),
    ).toBe(true)

    const bobSeat = screen.getByDisplayValue("Bob").closest("article") as HTMLElement
    fireEvent.change(within(bobSeat).getByRole("combobox", { name: "Deck (optional)" }), {
      target: { value: "Retired artifacts" },
    })
    fireEvent.click(within(bobSeat).getByRole("button", { name: "Move seat down" }))
    fireEvent.click(screen.getByRole("button", { name: "Log game" }))
    await waitFor(() => expect(navigate).toHaveBeenCalled())

    const atomicCalls = fetch.mock.calls.filter(
      ([url, init]) => url === "/api/discord/result-drafts/draft-uuid" && init?.method === "POST",
    )
    expect(atomicCalls).toHaveLength(1)
    const [, init] = atomicCalls[0]!
    expect(typeof init?.body).toBe("string")
    const payload = JSON.parse(init?.body as string).game
    expect(
      payload.seats.map((seat: { discord_id: string; result: string }) => [
        seat.discord_id,
        seat.result,
      ]),
    ).toEqual([
      ["discord-alice", "loss"],
      ["discord-cara", "loss"],
      ["discord-bob", "win"],
    ])
    expect(payload.seats[2]).toMatchObject({
      discord_id: "discord-bob",
      deck_id: 22,
      deck: null,
    })
    expect(
      fetch.mock.calls.some(
        ([url, request]) =>
          (url === "/api/players" || url === "/api/decks") && request?.method === "POST",
      ),
    ).toBe(false)
  })
})
