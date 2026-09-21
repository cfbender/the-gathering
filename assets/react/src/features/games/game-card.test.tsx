import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vite-plus/test"
import type { Game, Seat } from "./games"
import { GameCard } from "./game-card"

function seat(id: number, name: string, result: Seat["result"]): Seat {
  return {
    id,
    player_id: id,
    deck_id: id,
    seat: id,
    result,
    kills: null,
    mvp_card_id: null,
    mvp_card_name: null,
    mvp_art_crop_url: null,
    notes: null,
    player: { id, name, user_id: null, avatar_url: null, archived_at: null },
    deck: {
      id,
      player_id: id,
      name: `${name}'s deck`,
      commander_name: `${name}'s commander`,
      commander_card_id: null,
      commander_art_crop_url: `/art/${id}.jpg`,
      partner_card_id: null,
      partner_name: null,
      partner_art_crop_url: null,
      color_identity: "U",
      decklist_url: null,
      decklist_source: null,
      archived_at: null,
      skip_count: 0,
      included_for_play: true,
      player: null,
    },
  }
}

async function renderCard(seats: Seat[]) {
  const game: Game = {
    id: 42,
    played_at: "2026-09-19T18:00:00Z",
    duration_minutes: null,
    turns: null,
    win_condition: null,
    notes: null,
    source: "manual",
    external_id: null,
    created_by_user_id: null,
    seats,
  }
  const router = createRouter({
    routeTree: createRootRoute({ component: () => <GameCard game={game} /> }),
    history: createMemoryHistory({ initialEntries: ["/"] }),
  })
  render(<RouterProvider router={router} />)
  return screen.findByRole("link")
}

afterEach(cleanup)

describe("GameCard", () => {
  it("features the winning seat, not the first seat, without repeating it in the other players", async () => {
    const link = await renderCard([
      seat(1, "Alice", "loss"),
      seat(2, "Bob", "win"),
      seat(3, "Cara", "loss"),
    ])

    const winner = within(screen.getByRole("region", { name: "Winner" }))
    expect(winner.getByText("Bob")).toBeTruthy()
    expect(winner.getByText("Bob's commander")).toBeTruthy()
    expect(winner.getByText("Bob's deck")).toBeTruthy()
    expect(winner.getByRole("presentation").getAttribute("src")).toBe("/art/2.jpg")
    const others = within(screen.getByRole("list", { name: "Other players" }))
    expect(others.queryByText("Bob")).toBeNull()
    expect(
      others.getAllByRole("listitem").map((item) => item.querySelector("strong")?.textContent),
    ).toEqual(["Alice", "Cara"])
    expect(others.getAllByRole("presentation").map((image) => image.getAttribute("src"))).toEqual([
      "/art/1.jpg",
      "/art/3.jpg",
    ])
    expect(link.getAttribute("href")).toBe("/games/42")
    expect(screen.getByText("3 players")).toBeTruthy()
  })

  it("keeps every player on equal footing in a draw", async () => {
    await renderCard([seat(1, "Alice", "draw"), seat(2, "Bob", "draw")])

    expect(screen.queryByRole("region", { name: "Winner" })).toBeNull()
    expect(screen.getByRole("region", { name: "Draw" })).toBeTruthy()
    const players = within(screen.getByRole("list", { name: "Players" }))
    expect(players.getAllByRole("listitem")).toHaveLength(2)
    expect(players.getByText("Alice")).toBeTruthy()
    expect(players.getByText("Bob")).toBeTruthy()
  })

  it("preserves player and result information without decks or when artwork fails", async () => {
    await renderCard([
      { ...seat(1, "Alice", "win"), deck: null, deck_id: null },
      { ...seat(2, "Bob", "loss"), deck: null, deck_id: null },
      seat(3, "Cara", "loss"),
    ])

    const winner = within(screen.getByRole("region", { name: "Winner" }))
    expect(winner.getByText("Alice")).toBeTruthy()
    expect(winner.getByText("Unknown commander")).toBeTruthy()
    expect(winner.getByText("Unknown deck")).toBeTruthy()
    expect(winner.queryByRole("presentation")).toBeNull()
    const others = within(screen.getByRole("list", { name: "Other players" }))
    expect(others.getByText("Bob")).toBeTruthy()
    expect(others.getByText("Unknown commander")).toBeTruthy()
    const image = others.getByRole("presentation")
    fireEvent.error(image)
    expect(image.hidden).toBe(true)
    expect(others.getByText("Cara")).toBeTruthy()
  })
})
