import { describe, expect, it } from "vite-plus/test"
import {
  LAYOUTS,
  changeCommanderDamage,
  chooseFirstSeat,
  commanderKey,
  findLayout,
  gamePayload,
  landscape,
  lastStanding,
  newGame,
  passTurn,
  recordBlocker,
  setConceded,
  sit,
  startGame,
  type SeatDeck,
  type TabletopGame,
} from "./tabletop"

const deck = (id: number): SeatDeck => ({
  id,
  name: `Deck ${id}`,
  commanderName: "Atraxa",
  partnerName: null,
  artUrl: null,
  partnerArtUrl: null,
})

function playing(seatCount: number, format: TabletopGame["format"] = "commander") {
  let game = newGame({ format, seatCount, layoutId: "sides", startingLife: 40 })
  for (let index = 0; index < seatCount; index += 1) {
    game = sit(game, index, { kind: "player", id: 100 + index, name: `P${index}`, avatarUrl: null })
  }
  return startGame(game, new Date("2026-11-07T20:00:00Z"))
}

describe("passTurn", () => {
  it("goes clockwise, skips eliminated seats, and counts a round each time play passes the first player", () => {
    // Seat 2 starts; seat 0 is out. Order: 2 → 3 → (0 skipped) → 1 → 2 (round 2).
    let game = chooseFirstSeat(setConceded(playing(4), 0, true), 2)
    const seen: [number | null, number][] = []
    for (let step = 0; step < 4; step += 1) {
      game = passTurn(game)
      seen.push([game.activeSeat, game.turn])
    }
    expect(seen).toEqual([
      [3, 1],
      [1, 1],
      [2, 2],
      [3, 2],
    ])
  })

  it("starts a new round when the first player is eliminated and play passes their seat", () => {
    let game = chooseFirstSeat(playing(3), 0)
    game = passTurn(game) // seat 1
    game = passTurn(game) // seat 2
    game = setConceded(game, 0, true)
    game = passTurn(game)
    expect([game.activeSeat, game.turn]).toEqual([1, 2])
  })
})

describe("landscape", () => {
  it("turns every seat a quarter clockwise so it still faces the same table edge", () => {
    const portrait = findLayout(6, "ends")
    const turned = landscape(portrait)
    expect([turned.rows, turned.cols]).toEqual([2, 4])
    // The player at the bottom of the portrait board ends up on the left edge, spanning both rows.
    expect(turned.seats[0]).toEqual({ row: 1, col: 1, rowSpan: 2, colSpan: 1, rotation: 90 })
    // The top seat moves to the right edge.
    expect(turned.seats[3]).toEqual({ row: 1, col: 4, rowSpan: 2, colSpan: 1, rotation: 270 })
    // A left-side seat in the bottom half of the portrait board moves to the top, left of centre.
    expect(turned.seats[1]).toEqual({ row: 1, col: 2, rowSpan: 1, colSpan: 1, rotation: 180 })
  })

  it("covers every grid cell exactly once in every layout and orientation", () => {
    for (const layouts of Object.values(LAYOUTS)) {
      for (const layout of layouts.flatMap((portrait) => [portrait, landscape(portrait)])) {
        const cells = layout.seats.flatMap((seat) =>
          Array.from({ length: seat.rowSpan ?? 1 }, (_, r) =>
            Array.from({ length: seat.colSpan ?? 1 }, (_, c) => `${seat.row + r},${seat.col + c}`),
          ).flat(),
        )
        expect(new Set(cells).size, layout.id).toBe(layout.rows * layout.cols)
        expect(cells.length, layout.id).toBe(layout.rows * layout.cols)
      }
    }
  })
})

describe("commander damage", () => {
  it("also changes life, and a lethal amount eliminates the seat", () => {
    let game = playing(2)
    game = changeCommanderDamage(game, 0, commanderKey(1), 20)
    expect(game.seats[0]!.life).toBe(20)
    expect(lastStanding(game)).toBeNull()
    game = changeCommanderDamage(game, 0, commanderKey(1), 1)
    expect(lastStanding(game)).toBe(1)
    // Taking damage back off restores the same life, never below zero damage.
    game = changeCommanderDamage(game, 0, commanderKey(1), -30)
    expect(game.seats[0]!.commanderDamage[commanderKey(1)]).toBe(0)
    expect(game.seats[0]!.life).toBe(40)
  })

  it("is lethal at 16 in Pauper Commander", () => {
    const game = changeCommanderDamage(playing(2, "pauper_commander"), 1, commanderKey(0), 16)
    expect(lastStanding(game)).toBe(0)
  })
})

describe("recording", () => {
  it("records only when every seat is a playgroup member in a recorded format", () => {
    expect(recordBlocker(playing(3))).toBeNull()
    expect(recordBlocker(playing(2, "standard"))).toEqual({ kind: "format", format: "standard" })
    expect(recordBlocker(sit(playing(3), 1, { kind: "guest", name: "Sam" }))).toEqual({
      kind: "guests",
      names: ["Sam"],
      empty: false,
    })
    expect(recordBlocker(sit(playing(3), 2, null))).toMatchObject({ kind: "guests", empty: true })
  })

  it("lists seats in turn order from the first player, with decks and one winner", () => {
    let game = playing(3)
    game = sit(game, 2, game.seats[2]!.occupant, deck(7))
    game = chooseFirstSeat(game, 1)
    const payload = gamePayload(game, {
      winner: 2,
      durationMinutes: "45",
      turns: "",
      winCondition: "infinite_combo",
    })
    expect(payload).toMatchObject({
      format: "commander",
      played_at: "2026-11-07T20:00:00.000Z",
      duration_minutes: 45,
      turns: null,
    })
    expect(payload.seats).toEqual([
      { player_id: 101, deck_id: null, seat: 1, result: "loss" },
      { player_id: 102, deck_id: 7, seat: 2, result: "win" },
      { player_id: 100, deck_id: null, seat: 3, result: "loss" },
    ])
  })
})

describe("newGame", () => {
  it("keeps who sat where, and their decks, when the table is set up again", () => {
    let game = sit(playing(4), 1, { kind: "guest", name: "Sam" })
    game = sit(game, 0, game.seats[0]!.occupant, deck(3))
    const next = newGame(
      { format: "commander", seatCount: 3, layoutId: "head", startingLife: 30 },
      game,
    )
    expect(next.seats.map((seat) => [seat.occupant?.name, seat.deck?.id, seat.life])).toEqual([
      ["P0", 3, 30],
      ["Sam", undefined, 30],
      ["P2", undefined, 30],
    ])
  })
})
