import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, expect, it } from "vite-plus/test"
import { GameLengths } from "./game-lengths"

afterEach(cleanup)

it("preserves bin counts and average when switching the responsive chart to turns", () => {
  render(
    <GameLengths
      averageDuration={26}
      averageTurns={9}
      gameLengths={{
        durations: [
          { from: 0, to: 15, games: 1 },
          { from: 15, to: 30, games: 3 },
        ],
        turns: [{ from: 8, to: 10, games: 2 }],
        fastest_win: null,
        longest_game: null,
      }}
    />,
  )
  const duration = screen.getByRole("img", { name: "duration histogram" })
  expect(within(duration).getByTitle("15–29: 3 games").style.height).toBe("100%")
  expect(within(duration).getByTitle("0–14: 1 games").style.height).toBe("33.33333333333333%")
  expect(within(duration).getByText("AVG 26")).toBeTruthy()
  fireEvent.click(screen.getByRole("radio", { name: "Turns" }))
  const turns = screen.getByRole("img", { name: "turns histogram" })
  expect(within(turns).getByTitle("8–9: 2 games")).toBeTruthy()
  expect(within(turns).getByText("AVG 9")).toBeTruthy()
  expect(screen.queryByText("AVG 26")).toBeNull()
})
