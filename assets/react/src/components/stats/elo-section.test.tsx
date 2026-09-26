import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { EloSection } from "./elo-section"

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    children,
    params,
    to: _to,
    ...props
  }: {
    children: ReactNode
    to: string
    params: { playerId: string }
  }) => (
    <a href={`/players/${params.playerId}`} {...props}>
      {children}
    </a>
  ),
}))

afterEach(cleanup)

const player = (id: number, name: string, rating: number) => ({
  id,
  name,
  rating,
  peak: rating,
  games: 5,
  history: [{ date: `2026-01-0${id}`, rating }],
})

const players = [player(1, "Alice", 1040), player(2, "Bob", 980), player(3, "Cara", 1010)]

const lineOpacities = (container: HTMLElement) =>
  Object.fromEntries(
    [...container.querySelectorAll("g[data-player-id]")].map((group) => [
      group.getAttribute("data-player-id"),
      group.querySelector("[data-elo-line]")?.getAttribute("stroke-opacity"),
    ]),
  )

it("highlights a player's line from the current ratings pane", () => {
  const { container } = render(<EloSection players={players} />)
  const bob = screen.getByRole("link", { name: /Bob/ })

  fireEvent.mouseEnter(bob)
  expect(lineOpacities(container)).toEqual({ "1": "0.15", "2": "1", "3": "0.15" })

  fireEvent.mouseLeave(bob.parentElement!)
  expect(lineOpacities(container)).toEqual({ "1": "1", "2": "1", "3": "1" })

  fireEvent.focus(bob)
  expect(lineOpacities(container)["2"]).toBe("1")
  expect(lineOpacities(container)["1"]).toBe("0.15")
  fireEvent.blur(bob)
  expect(lineOpacities(container)["1"]).toBe("1")
})

it("highlights a player's line from the chart legend", () => {
  const { container } = render(<EloSection players={players} />)

  fireEvent.mouseEnter(screen.getByText("Cara", { selector: "li span" }).closest("li")!)
  expect(lineOpacities(container)).toEqual({ "1": "0.15", "2": "0.15", "3": "1" })
})

it("highlights a player's line when hovering the line itself", () => {
  const { container } = render(<EloSection players={players} />)
  const aliceLine = container.querySelector('g[data-player-id="1"]')!

  fireEvent.mouseEnter(aliceLine)
  expect(lineOpacities(container)).toEqual({ "1": "1", "2": "0.15", "3": "0.15" })

  fireEvent.mouseLeave(aliceLine)
  expect(lineOpacities(container)).toEqual({ "1": "1", "2": "1", "3": "1" })
})
