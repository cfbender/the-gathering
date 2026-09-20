import { describe, expect, it } from "vite-plus/test"
import { moveSeat, resultsForSeats, type DraftSeat } from "./game-form-logic"

const seats = ["Alice", "Bob", "Cara"].map(
  (playerName, index) => ({ draftId: `seat-${index}`, playerName }) as DraftSeat,
)

describe("game form seat and winner logic", () => {
  it("moves turn order without dropping a seat and respects either boundary", () => {
    expect(moveSeat(seats, 1, -1).map((seat) => seat.playerName)).toEqual(["Bob", "Alice", "Cara"])
    expect(moveSeat(seats, 0, -1)).toBe(seats)
    expect(moveSeat(seats, 2, 1)).toBe(seats)
  })

  it("produces exactly one winner or an all-draw result", () => {
    expect(resultsForSeats(seats, "seat-1")).toEqual(["loss", "win", "loss"])
    expect(resultsForSeats(seats, null)).toEqual(["draw", "draw", "draw"])
  })
})
