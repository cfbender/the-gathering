import { cleanup, render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { KillStats } from "./kill-stats"

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children, params }: { children: ReactNode; params: { playerId: string } }) => (
    <a href={`/players/${params.playerId}`}>{children}</a>
  ),
}))

afterEach(cleanup)

it("distinguishes unrecorded kills from recorded zero kills", () => {
  const { rerender } = render(
    <KillStats stats={{ total: 0, recorded_seats: 0, total_seats: 9, players: [] }} />,
  )
  expect(screen.getByText("—")).toBeTruthy()
  expect(screen.getByText("No kill counts in this range yet.")).toBeTruthy()

  rerender(
    <KillStats
      stats={{
        total: 0,
        recorded_seats: 2,
        total_seats: 9,
        players: [{ id: 7, name: "Alice", kills: 0, recorded_games: 2, average: 0 }],
      }}
    />,
  )
  expect(screen.queryByText("—")).toBeNull()
  expect(screen.getByText(/Counts recorded for 2 of 9 player appearances/)).toBeTruthy()
  const player = screen.getByRole("link", { name: /^Alice/ })
  expect(player.getAttribute("href")).toBe("/players/7")
  expect(within(player).getByText("0")).toBeTruthy()
  expect(within(player).getByText("0.00")).toBeTruthy()
})
