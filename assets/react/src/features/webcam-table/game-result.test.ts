import { describe, expect, it } from "vite-plus/test"
import { buildGamePayload, suggestedWinner } from "./game-result"
import { EMPTY_COUNTERS } from "./seat-counters"

const seat = { ...EMPTY_COUNTERS, life: 40, camera_off: false, eliminated: false, joined_at: 1_000 }
const participants = [
  {
    ...seat,
    peer_id: "peer-a",
    player_id: 12,
    player_name: "Alice",
    deck_id: 41,
    deck_name: "Birds",
  },
  { ...seat, peer_id: "peer-b", player_id: 27, player_name: "Bob" },
]

describe("buildGamePayload", () => {
  it("records all ten seats in supplied turn order with the last player winning", () => {
    const seats = Array.from({ length: 10 }, (_, index) => ({
      ...participants[0]!,
      player_id: 100 - index,
      peer_id: `peer-${index}`,
    }))
    const payload = buildGamePayload(seats, {
      playedAt: new Date("2026-09-22T19:30:00Z"),
      winner: "peer-9",
      duration: "73",
      turns: "11",
      winCondition: "",
      notes: "",
    })
    expect(payload.game.seats.map((seat) => seat.player_id)).toEqual([
      100, 99, 98, 97, 96, 95, 94, 93, 92, 91,
    ])
    expect(payload.game.seats.map((seat) => seat.seat)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
    expect(payload.game.seats.map((seat) => seat.result)).toEqual([
      "loss",
      "loss",
      "loss",
      "loss",
      "loss",
      "loss",
      "loss",
      "loss",
      "loss",
      "win",
    ])
  })

  it("suggests the sole remaining player and records eliminated losses in seat order", () => {
    const seated = participants.map((player, index) => ({ ...player, eliminated: index === 1 }))
    expect(suggestedWinner(seated)).toBe("peer-a")
    expect(suggestedWinner(participants)).toBe("")
    expect(suggestedWinner(seated.map((player) => ({ ...player, eliminated: true })))).toBe("")
    expect(suggestedWinner([participants[0]!])).toBe("")
    const payload = buildGamePayload(seated, {
      playedAt: new Date("2026-09-22T19:30:00Z"),
      winner: suggestedWinner(seated),
      duration: "73",
      turns: "",
      winCondition: "",
      notes: "",
    })
    expect(payload.game.seats).toEqual([
      { player_id: 12, deck_id: 41, seat: 1, result: "win" },
      { player_id: 27, deck_id: null, seat: 2, result: "loss" },
    ])
  })

  it("records the selected winner, consecutive seats, and nullable deck through the normal shape", () => {
    const payload = buildGamePayload(participants, {
      playedAt: new Date("2026-09-22T19:30:00Z"),
      winner: "peer-b",
      duration: "73",
      turns: "11",
      winCondition: "commander_damage",
      notes: "  webcam table  ",
    })

    expect(payload).toEqual({
      game: {
        format: "commander",
        played_at: "2026-09-22T19:30:00.000Z",
        duration_minutes: 73,
        turns: 11,
        win_condition: "commander_damage",
        notes: "webcam table",
        seats: [
          { player_id: 12, deck_id: 41, seat: 1, result: "loss" },
          { player_id: 27, deck_id: null, seat: 2, result: "win" },
        ],
      },
    })
  })

  it("marks every seat as a draw", () => {
    const payload = buildGamePayload(participants, {
      playedAt: new Date("2026-09-22T19:30:00Z"),
      winner: "draw",
      duration: "",
      turns: "",
      winCondition: "",
      notes: "",
    })

    expect(payload.game.seats.map((seat) => seat.result)).toEqual(["draw", "draw"])
  })
})
