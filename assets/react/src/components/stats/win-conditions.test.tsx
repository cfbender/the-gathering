import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { PlayerWinConditions, WinConditions } from "./win-conditions"

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    children,
    to,
    search,
  }: {
    children: ReactNode
    to: string
    search: Record<string, string | number>
  }) => (
    <a
      href={`${to}?${new URLSearchParams(Object.entries(search).map(([k, v]) => [k, String(v)]))}`}
    >
      {children}
    </a>
  ),
}))

afterEach(cleanup)

describe("win conditions", () => {
  it("uses known games as the share denominator and reports coverage", () => {
    render(
      <WinConditions
        stats={{
          recorded_games: 3,
          total_games: 8,
          conditions: [
            { condition: "combat_damage", games: 2 },
            { condition: "mill", games: 1 },
          ],
        }}
      />,
    )

    expect(screen.getByText(/Known for 3 of 8 games/)).toBeTruthy()
    expect(screen.getByText("2 · 67%")).toBeTruthy()
    expect(screen.getByText("1 · 33%")).toBeTruthy()
  })

  it("shows every tied favorite without selecting a less frequent condition", () => {
    render(
      <WinConditions
        perspective="wins"
        stats={{
          recorded_games: 5,
          total_games: 7,
          conditions: [
            { condition: "alternate_win_con", games: 2 },
            { condition: "infinite_combo", games: 2 },
            { condition: "mill", games: 1 },
          ],
        }}
      />,
    )

    expect(screen.getByText("On-card Alternate Win Con / Infinite Combo")).toBeTruthy()
    expect(screen.getByText("Tied for most frequent across their wins")).toBeTruthy()
    expect(screen.getByText(/Known for 5 of 7 wins/)).toBeTruthy()
  })

  it("does not invent a favorite when wins have no known condition", () => {
    render(
      <WinConditions
        perspective="wins"
        stats={{ recorded_games: 0, total_games: 4, conditions: [] }}
      />,
    )
    expect(screen.getByText("No known win conditions for this player's wins yet.")).toBeTruthy()
    expect(screen.queryByRole("list")).toBeNull()
  })

  it("switches between distinct winning and losing conditions and back", () => {
    render(
      <PlayerWinConditions
        wins={{ recorded_games: 2, total_games: 3, conditions: [{ condition: "mill", games: 2 }] }}
        losses={{
          recorded_games: 1,
          total_games: 5,
          conditions: [{ condition: "poison", games: 1 }],
        }}
      />,
    )
    expect(screen.getByRole("heading", { name: "Favorite win con" })).toBeTruthy()
    fireEvent.click(screen.getByRole("radio", { name: "Losses" }))
    expect(screen.getByRole("heading", { name: "Win cons lost to" })).toBeTruthy()
    expect(screen.getByText(/Known for 1 of 5 losses/)).toBeTruthy()
    expect(screen.getAllByText("Poison")).toHaveLength(2)
    expect(screen.queryByText("Mill")).toBeNull()
    fireEvent.click(screen.getByRole("radio", { name: "Wins" }))
    expect(screen.getAllByText("Mill")).toHaveLength(2)
    expect(screen.getByText(/Known for 2 of 3 wins/)).toBeTruthy()
  })

  it("links a profile's conditions to that player's wins or losses within the range", () => {
    const stats = {
      recorded_games: 1,
      total_games: 1,
      conditions: [{ condition: "mill" as const, games: 1 }],
    }
    render(
      <PlayerWinConditions
        wins={stats}
        losses={{ ...stats, conditions: [{ condition: "poison" as const, games: 1 }] }}
        games={{ date_from: "2026-03-01", player_id: 7 }}
      />,
    )
    expect(screen.getByRole("link", { name: /Mill/ }).getAttribute("href")).toBe(
      "/games?winner_id=7&win_condition=mill&date_from=2026-03-01",
    )
    fireEvent.click(screen.getByRole("radio", { name: "Losses" }))
    expect(screen.getByRole("link", { name: /Poison/ }).getAttribute("href")).toBe(
      "/games?player_id=7&win_condition=poison&player_result=loss&date_from=2026-03-01",
    )
  })
})
