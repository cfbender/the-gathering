import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { EloChart } from "./elo-chart"

afterEach(cleanup)

const series = [
  {
    id: 1,
    name: "Alice",
    rating: 1040,
    start: 1000,
    history: [{ date: "2026-01-01", rating: 1040 }],
  },
  { id: 2, name: "Bob", rating: 980, start: 1000, history: [{ date: "2026-01-02", rating: 980 }] },
  {
    id: 3,
    name: "Cara",
    rating: 1010,
    start: 1000,
    history: [{ date: "2026-01-03", rating: 1010 }],
  },
]

const lineOpacities = (container: HTMLElement) =>
  Object.fromEntries(
    [...container.querySelectorAll("g[data-player-id]")].map((group) => [
      group.getAttribute("data-player-id"),
      group.querySelector("[data-elo-line]")?.getAttribute("stroke-opacity"),
    ]),
  )

it("dims the other lines and draws the highlighted line on top", () => {
  const { container, rerender } = render(<EloChart series={series} />)
  expect(lineOpacities(container)).toEqual({ "1": "1", "2": "1", "3": "1" })

  rerender(<EloChart series={series} highlightedId={2} />)
  expect(lineOpacities(container)).toEqual({ "1": "0.15", "2": "1", "3": "0.15" })
  expect(
    container.querySelector("g[data-player-id]:last-of-type")?.getAttribute("data-player-id"),
  ).toBe("2")

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

it("reports hover changes on the lines themselves", () => {
  const onHighlightChange = vi.fn()
  const { container } = render(<EloChart series={series} onHighlightChange={onHighlightChange} />)
  const bobLine = container.querySelector('g[data-player-id="2"]')!

  fireEvent.mouseEnter(bobLine)
  expect(onHighlightChange).toHaveBeenLastCalledWith(2)

  fireEvent.mouseLeave(bobLine)
  expect(onHighlightChange).toHaveBeenLastCalledWith(null)
})

it("shows each legend change from the rating carried into the range", () => {
  render(
    <EloChart
      series={[
        {
          id: 1,
          name: "Alice",
          rating: 1090,
          start: 1100,
          history: [
            { date: "2026-04-01", rating: 1100 },
            { date: "2026-04-02", rating: 1090 },
          ],
        },
      ]}
    />,
  )

  expect(screen.getByText("Alice").closest("li")?.textContent).toContain("1090 (-10)")
})

it("shows each player's rating on the date under the pointer", () => {
  const { container } = render(
    <EloChart
      series={[
        {
          id: 1,
          name: "Alice",
          rating: 1030,
          start: 1000,
          history: [
            { date: "2026-01-01", rating: 1016 },
            { date: "2026-01-11", rating: 1030 },
          ],
        },
        {
          id: 2,
          name: "Bob",
          rating: 990,
          start: 1000,
          history: [{ date: "2026-01-06", rating: 990 }],
        },
      ]}
    />,
  )
  const plot = container.querySelector("svg")!.parentElement!
  plot.getBoundingClientRect = () => ({ left: 0, width: 600 }) as DOMRect

  // x = 310 is the middle of the plot: Jan 6, after Alice's first game and on Bob's only one.
  fireEvent.pointerMove(plot, { clientX: 310 })
  const tooltip = container.querySelector("[data-elo-tooltip]")!
  expect(tooltip.textContent).toContain(
    new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(2026, 0, 6)),
  )
  expect([...tooltip.querySelectorAll("li")].map((row) => row.textContent)).toEqual([
    "Alice1016",
    "Bob990",
  ])

  // Near the start only Alice has played.
  fireEvent.pointerMove(plot, { clientX: 20 })
  expect(
    [...container.querySelectorAll("[data-elo-tooltip] li")].map((row) => row.textContent),
  ).toEqual(["Alice1016"])

  fireEvent.pointerLeave(plot)
  expect(container.querySelector("[data-elo-tooltip]")).toBeNull()
})
