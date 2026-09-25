import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { EloChart } from "./elo-chart"

afterEach(cleanup)

const series = [
  { id: 1, name: "Alice", rating: 1040, history: [{ date: "2026-01-01", rating: 1040 }] },
  { id: 2, name: "Bob", rating: 980, history: [{ date: "2026-01-02", rating: 980 }] },
  { id: 3, name: "Cara", rating: 1010, history: [{ date: "2026-01-03", rating: 1010 }] },
]

const lineOpacities = (container: HTMLElement) =>
  Object.fromEntries(
    [...container.querySelectorAll("polyline")].map((line) => [
      line.dataset.playerId,
      line.getAttribute("stroke-opacity"),
    ]),
  )

it("dims the other lines and draws the highlighted line on top", () => {
  const { container, rerender } = render(<EloChart series={series} />)
  expect(lineOpacities(container)).toEqual({ "1": "1", "2": "1", "3": "1" })

  rerender(<EloChart series={series} highlightedId={2} />)
  expect(lineOpacities(container)).toEqual({ "1": "0.15", "2": "1", "3": "0.15" })
  expect(container.querySelector("polyline:last-of-type")?.getAttribute("data-player-id")).toBe("2")

  rerender(<EloChart series={series} highlightedId={99} />)
  expect(lineOpacities(container)).toEqual({ "1": "1", "2": "1", "3": "1" })
})

it("reports legend hover changes", () => {
  const onHighlightChange = vi.fn()
  render(<EloChart series={series} onHighlightChange={onHighlightChange} />)

  fireEvent.mouseEnter(screen.getByText("Bob").closest("li")!)
  expect(onHighlightChange).toHaveBeenLastCalledWith(2)

  fireEvent.mouseLeave(screen.getByRole("list"))
  expect(onHighlightChange).toHaveBeenLastCalledWith(null)
})
