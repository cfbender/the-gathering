import { describe, expect, it } from "vite-plus/test"
import { EMPTY_COUNTERS } from "./seat-counters"
import { unattackableSeats, turnId } from "./game-modes"
import { nextActiveSeat, turnDisplay, EMPTY_TURNS } from "./turns"
import { buildGamePayload, suggestedWinner } from "./game-result"

const seats = [8, 3, 17, 5, 2].map((id) => ({
  ...EMPTY_COUNTERS,
  player_id: id,
  peer_id: `peer-${id}`,
  player_name: `Player ${id}`,
  life: 40,
  camera_off: false,
  eliminated: false,
  joined_at: id,
}))
const out = (...indices: number[]) =>
  seats.map((seat, index) => ({ ...seat, eliminated: indices.includes(index) }))

describe("Five Star original-seat neighbours", () => {
  it("wraps at seat 1 and drops eliminated neighbours", () => {
    expect(unattackableSeats(seats, 8)).toEqual(["peer-2", "peer-3"])
    expect(unattackableSeats(out(2), 8)).toEqual(["peer-2", "peer-3"])
    expect(unattackableSeats(out(1), 8)).toEqual(["peer-2"])
    expect(unattackableSeats(out(4), 8)).toEqual(["peer-3"])
  })
  it("lifts every restriction once three players remain", () => {
    expect(unattackableSeats(out(2, 3), 8)).toEqual([])
    expect(unattackableSeats(out(1, 2), 8)).toEqual([])
    expect(unattackableSeats(out(3, 4), 17)).toEqual([])
  })
  it("wraps at seat 5 and does not give spectators badges", () => {
    expect(unattackableSeats(out(2), 2)).toEqual(["peer-5", "peer-8"])
    expect(unattackableSeats(out(1, 2), 2)).toEqual([])
    expect(unattackableSeats(seats, 999)).toEqual([])
    expect(unattackableSeats(seats.slice(0, 4), 8)).toEqual([])
  })
})

describe("2HG turns and results", () => {
  it("skips the teammate and displays the same authoritative count and time", () => {
    const four = seats.slice(0, 4)
    expect(nextActiveSeat(four, 3, "two_headed_giant")?.player_id).toBe(17)
    expect(nextActiveSeat(four, 5, "two_headed_giant")?.player_id).toBe(8)
    expect(turnId(four, 5, "two_headed_giant")).toBe(17)
    const turns = {
      ...EMPTY_TURNS,
      active_player_id: 17,
      counts: { 8: 2, 17: 3 },
      elapsed_ms: { 17: 9000 },
      started_elapsed_ms: 14000,
    }
    expect(turnDisplay(turns, 5, 19000, four, "two_headed_giant")).toEqual({
      count: 3,
      milliseconds: 14000,
    })
    expect(turnDisplay(turns, 3, 19000, four, "two_headed_giant")).toEqual({
      count: 2,
      milliseconds: 0,
    })
  })
  it("suggests the last surviving team and records two wins or four draws", () => {
    const four = out(0, 1, 2).slice(0, 4)
    const mode = "two_headed_giant"
    expect(suggestedWinner(four, mode)).toBe("peer-17")
    expect(suggestedWinner(seats.slice(0, 4), mode)).toBe("")
    const details = {
      playedAt: new Date("2026-09-23T00:00:00Z"),
      winner: "peer-17",
      duration: "",
      turns: "",
      winCondition: "",
      notes: "",
    }
    const result = buildGamePayload(four, details, mode).game
    expect(result.format).toBe(mode)
    expect(result.seats.map((seat) => seat.result)).toEqual(["loss", "loss", "win", "win"])
    expect(
      buildGamePayload(four, { ...details, winner: "draw" }, mode).game.seats.map(
        (seat) => seat.result,
      ),
    ).toEqual(["draw", "draw", "draw", "draw"])
  })
})
