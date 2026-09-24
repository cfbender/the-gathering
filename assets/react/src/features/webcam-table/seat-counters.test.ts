import { describe, expect, it } from "vite-plus/test"
import { EMPTY_COUNTERS, changeCounter, commanderNames, counterWarning } from "./seat-counters"

describe("seat counters", () => {
  it("resolves both commanders, including backgrounds, from a selected deck", () => {
    expect(
      commanderNames({ commander_name: "Abdel Adrian", partner_name: "Candlekeep Sage" }),
    ).toEqual(["Abdel Adrian", "Candlekeep Sage"])
    expect(commanderNames({ commander_name: "Kangee", partner_name: null })).toEqual(["Kangee"])
    expect(commanderNames(undefined)).toEqual([])
  })

  it("compounds deltas without mutating earlier state and clamps at zero and the server limit", () => {
    const first = changeCounter(EMPTY_COUNTERS, { kind: "poison" }, 7)
    expect(changeCounter(first, { kind: "poison" }, 3).poison).toBe(10)
    expect(changeCounter(first, { kind: "poison" }, -8).poison).toBe(0)
    expect(changeCounter(first, { kind: "rad" }, 1000).rad).toBe(999)
    expect(first.poison).toBe(7)
    expect(EMPTY_COUNTERS.poison).toBe(0)
  })

  it("keeps partner casts and damage separate, even when opponents share a commander", () => {
    let state = changeCounter(
      EMPTY_COUNTERS,
      { kind: "damage", playerId: 2, commander: "Tymna" },
      20,
    )
    state = changeCounter(state, { kind: "damage", playerId: 3, commander: "Tymna" }, 8)
    state = changeCounter(state, { kind: "damage", playerId: 2, commander: "Thrasios" }, 5)
    state = changeCounter(state, { kind: "casts", commander: "Tymna" }, 3)
    state = changeCounter(state, { kind: "casts", commander: "Thrasios" }, 1)
    expect(state.commander_damage).toEqual({ "2": { Tymna: 20, Thrasios: 5 }, "3": { Tymna: 8 } })
    expect(state.commander_casts).toEqual({ Tymna: 3, Thrasios: 1 })
    expect(counterWarning(state)).toBe(false)
    expect(
      counterWarning(
        changeCounter(state, { kind: "damage", playerId: 2, commander: "Thrasios" }, 16),
      ),
    ).toBe(true)
    expect(counterWarning({ ...state, poison: 9 })).toBe(false)
    expect(counterWarning({ ...state, poison: 10 })).toBe(true)
  })
})
