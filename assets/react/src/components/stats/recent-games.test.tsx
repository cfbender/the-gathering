import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { RecentGames } from "./recent-games"

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children, params }: { children: ReactNode; params?: { gameId: string } }) => (
    <a href={params ? `/games/${params.gameId}` : "/games"}>{children}</a>
  ),
}))

afterEach(cleanup)

it("shows commander portraits with accessible names and a fallback for absent or broken art", () => {
  render(
    <RecentGames
      games={[
        {
          id: 4,
          played_at: "2026-09-01T00:00:00Z",
          players: 2,
          duration_minutes: 43,
          turns: null,
          result: null,
          winner: { id: 1, name: "Alice" },
          commanders: [
            { player_name: "Alice", name: "Kangee", art_crop_url: "/kangee.jpg", winner: true },
            { player_name: "Bob", name: null, art_crop_url: null, winner: false },
          ],
        },
      ]}
    />,
  )
  expect(screen.getByRole("link", { name: /Alice won/ }).getAttribute("href")).toBe("/games/4")
  const winner = screen.getByRole("listitem", { name: "Alice: Kangee (winner)" })
  const art = within(winner).getByRole("presentation")
  expect(art.getAttribute("src")).toBe("/kangee.jpg")
  fireEvent.error(art)
  expect(art.hidden).toBe(true)
  expect(screen.getByRole("listitem", { name: "Bob: Unknown commander" })).toBeTruthy()
})
