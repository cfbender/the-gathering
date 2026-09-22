import { describe, expect, it } from "vite-plus/test"
import { buildGamePayload } from "./game-result"

const seat = { life: 40, camera_off: false, joined_at: 1_000 }
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
