import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { useState } from "react"
import { afterEach, describe, expect, it } from "vite-plus/test"
import { GameTable } from "./game-table"
import { GameViewToggle, type GameView } from "./game-view-toggle"
import type { Game, Seat } from "./games"

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
      partner_name: "Partner commander",
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

const game: Game = {
  id: 42,
  format: "commander",
  played_at: "2026-09-19T18:00:00Z",
  duration_minutes: 83,
  turns: 7,
  win_condition: null,
  notes: null,
  source: "manual",
  external_id: null,
  created_by_user_id: null,
  seats: [seat(1, "Alice", "loss"), seat(2, "Bob", "win"), seat(3, "Cara", "loss")],
}

async function renderTable(games: Game[]) {
  const router = createRouter({
    routeTree: createRootRoute({ component: () => <GameTable games={games} /> }),
    history: createMemoryHistory({ initialEntries: ["/"] }),
  })
  render(<RouterProvider router={router} />)
  return screen.findByRole("table")
}

afterEach(cleanup)

describe("GameTable", () => {
  it("labels Five Star and shows both winners for 2HG", async () => {
    await renderTable([
      { ...game, format: "five_star" },
      {
        ...game,
        id: 43,
        format: "two_headed_giant",
        seats: [seat(1, "Alice", "win"), seat(2, "Bob", "win"), seat(3, "Cara", "loss")],
      },
    ])
    expect(screen.getByText("Five Star")).toBeTruthy()
    expect(screen.getByText("2HG")).toBeTruthy()
    expect(screen.getByRole("cell", { name: /Winner:.*Alice \+ Bob/ })).toBeTruthy()
  })

  it("keeps seat order, marks the actual winner, and links each date to its game", async () => {
    await renderTable([game])
    expect(screen.getAllByRole("columnheader").map((header) => header.textContent)).toEqual([
      "Played",
      "Players",
      "Result",
      "Turns",
      "Duration",
    ])
    const players = within(screen.getByRole("list", { name: "3 players" }))
    expect(
      players.getAllByRole("button").map((button) => button.getAttribute("aria-label")),
    ).toEqual([
      "Alice — Loss. View commander details",
      "Bob — Winner. View commander details",
      "Cara — Loss. View commander details",
    ])
    expect(players.getAllByRole("presentation").map((image) => image.getAttribute("src"))).toEqual([
      "/art/1.jpg",
      "/art/2.jpg",
      "/art/3.jpg",
    ])
    expect(screen.getByRole("cell", { name: /Winner:\s*Bob/ })).toBeTruthy()
    expect(screen.getByRole("cell", { name: "7 turns" })).toBeTruthy()
    expect(screen.getByRole("cell", { name: "83 min" })).toBeTruthy()
    expect(screen.getByRole("link").getAttribute("href")).toBe("/games/42")

    fireEvent.click(players.getByRole("button", { name: /Bob/ }))
    const details = within(await screen.findByRole("dialog", { name: "Bob's commander details" }))
    expect(details.getByText("Bob's commander")).toBeTruthy()
    expect(details.getByText("Partner commander")).toBeTruthy()
    expect(details.getByText("Bob's deck")).toBeTruthy()
    expect(details.getByText("Winner · Seat 2")).toBeTruthy()
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  it("shows draws without a winner and distinguishes missing metrics from zero", async () => {
    await renderTable([
      {
        ...game,
        turns: null,
        duration_minutes: null,
        seats: [seat(1, "Alice", "draw"), seat(2, "Bob", "draw")],
      },
      { ...game, id: 43, turns: 0, duration_minutes: 0 },
    ])
    expect(screen.getByRole("cell", { name: "Draw" })).toBeTruthy()
    const drawPlayers = within(screen.getByRole("list", { name: "2 players" }))
    expect(drawPlayers.queryByRole("button", { name: /Winner/ })).toBeNull()
    expect(drawPlayers.getAllByRole("button", { name: /Draw/ })).toHaveLength(2)
    expect(screen.getAllByLabelText("Not recorded")).toHaveLength(2)
    expect(screen.getByRole("cell", { name: "0 turns" })).toBeTruthy()
    expect(screen.getByRole("cell", { name: "0 min" })).toBeTruthy()
  })

  it("retains player identity and details when decks or artwork are unavailable", async () => {
    await renderTable([
      {
        ...game,
        seats: [{ ...seat(1, "Alice", "win"), deck: null, deck_id: null }, seat(2, "Bob", "loss")],
      },
    ])
    const alice = screen.getByRole("button", { name: /Alice/ })
    expect(within(alice).queryByRole("presentation")).toBeNull()
    fireEvent.click(alice)
    const details = within(await screen.findByRole("dialog"))
    expect(details.getByText("Unknown commander")).toBeTruthy()
    expect(details.getByText("Unknown deck")).toBeTruthy()
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" })
    const bob = screen.getByRole("button", { name: /Bob/ })
    const image = within(bob).getByRole("presentation")
    fireEvent.error(image)
    expect(image.hidden).toBe(true)
    expect(within(bob).getByText("Bob")).toBeTruthy()
  })
})

it("switches views without allowing the selected view to be deselected", () => {
  function ViewControl() {
    const [view, setView] = useState<GameView>("cards")
    return <GameViewToggle value={view} onChange={setView} />
  }
  render(<ViewControl />)
  const cards = screen.getByRole("radio", { name: "Cards" })
  const table = screen.getByRole("radio", { name: "Table" })
  expect(cards.getAttribute("aria-checked")).toBe("true")
  fireEvent.click(table)
  expect(table.getAttribute("aria-checked")).toBe("true")
  expect(cards.getAttribute("aria-checked")).toBe("false")
  fireEvent.click(table)
  expect(table.getAttribute("aria-checked")).toBe("true")
  fireEvent.click(cards)
  expect(cards.getAttribute("aria-checked")).toBe("true")
})
