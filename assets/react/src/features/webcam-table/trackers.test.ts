import { describe, expect, it } from "vite-plus/test"
import {
  adjustCounter,
  combatRows,
  describeConditions,
  formatBuff,
  parseList,
  parseSavedTrackers,
  receivedTrackers,
  sharedTrackers,
  type CombatEffect,
} from "./trackers"

function effect(
  name: string,
  power: number,
  toughness: number,
  conditions: string[] = [],
  keywords: string[] = [],
): CombatEffect {
  return { id: name, name, power, toughness, conditions, keywords }
}

function summary(effects: CombatEffect[]) {
  return combatRows(effects).map((row) => {
    const buff = formatBuff(row.power, row.toughness)
    return row.keywords.length
      ? `${row.label} ${buff} ${row.keywords.join(", ")}`
      : `${row.label} ${buff}`
  })
}

describe("combatRows", () => {
  it("combines anthems for every kind of creature they overlap on", () => {
    expect(
      summary([
        effect("Caretaker's Talent", 2, 2, ["Token"]),
        effect("Warren Warleader", 1, 1, ["Attacking"]),
        effect("Intangible Virtue", 1, 1, ["Token"], ["vigilance"]),
      ]),
    ).toEqual([
      "Attacking creatures +1/+1",
      "Token creatures +3/+3 vigilance",
      "Attacking token creatures +4/+4 vigilance",
    ])
  })

  it("lists an unconditional anthem as all creatures and adds it to every row", () => {
    expect(
      summary([
        effect("Glorious Anthem", 1, 1),
        effect("Caretaker's Talent", 2, 2, ["Token"]),
        effect("Warren Warleader", 1, 1, ["Attacking"]),
      ]),
    ).toEqual([
      "All creatures +1/+1",
      "Attacking creatures +2/+2",
      "Token creatures +3/+3",
      "Attacking token creatures +4/+4",
    ])
  })

  it("omits combinations where a condition changes nothing", () => {
    // A buff needing both conditions only shows up with both; alone, "attacking" adds nothing.
    expect(
      summary([
        effect("Caretaker's Talent", 2, 2, ["Token"]),
        effect("Attacking tokens", 1, 1, ["Token", "Attacking"]),
      ]),
    ).toEqual(["Token creatures +2/+2", "Attacking token creatures +3/+3"])
  })

  it("hides totals that cancel out and treats conditions case-insensitively", () => {
    expect(
      summary([
        effect("Anthem", 1, 1, ["token"]),
        effect("Curse", -1, -1, ["TOKEN"]),
        effect("Elvish Archdruid", 1, 1, ["Elf"]),
      ]),
    ).toEqual(["Elf creatures +1/+1"])
  })

  it("keeps a keyword-only effect and shows negative and asymmetric buffs", () => {
    expect(
      summary([
        effect("Akroma's Memorial", 0, 0, [], ["flying", "first strike"]),
        effect("Night of Souls' Betrayal", -1, -1),
        effect("Rally", 2, 0, ["Attacking"]),
      ]),
    ).toEqual([
      "All creatures -1/-1 flying, first strike",
      "Attacking creatures +1/-1 flying, first strike",
    ])
  })

  it("returns nothing without effects", () => {
    expect(combatRows([])).toEqual([])
    expect(combatRows([effect("Nothing", 0, 0, ["Token"])])).toEqual([])
  })
})

describe("describeConditions", () => {
  it("orders state, custom types, then tokenness", () => {
    expect(describeConditions(["Token", "Elf", "Flying", "Attacking"])).toBe(
      "Attacking flying Elf token creatures",
    )
    expect(describeConditions(["Nontoken", "Blocking"])).toBe("Blocking nontoken creatures")
    expect(describeConditions([])).toBe("All creatures")
  })
})

describe("sharing", () => {
  const trackers = {
    counters: [
      { id: "a", label: "Lands", value: 7, shared: true },
      { id: "b", label: "Storm", value: 3, shared: false },
    ],
    effects: [effect("Anthem", 1, 1)],
    shareEffects: false,
  }

  it("publishes only shared counters and the buff list only when shared", () => {
    expect(sharedTrackers(trackers)).toEqual({
      custom_counters: [{ id: "a", label: "Lands", value: 7 }],
      combat_effects: [],
    })
    expect(sharedTrackers({ ...trackers, shareEffects: true }).combat_effects).toEqual(
      trackers.effects,
    )
  })

  it("reads another seat's published trackers, tolerating seats without any", () => {
    expect(receivedTrackers({ custom_counters: [{ id: "a", label: "Lands", value: 7 }] })).toEqual({
      counters: [{ id: "a", label: "Lands", value: 7, shared: true }],
      effects: [],
      shareEffects: true,
    })
    expect(receivedTrackers(undefined).counters).toEqual([])
  })

  it("clamps counters to 0–100", () => {
    expect(adjustCounter(trackers, "a", 200).counters[0]?.value).toBe(100)
    expect(adjustCounter(trackers, "b", -5).counters[1]?.value).toBe(0)
    expect(adjustCounter(trackers, "b", -5).counters[0]?.value).toBe(7)
  })
})

describe("parsing", () => {
  it("splits typed lists and drops duplicates", () => {
    expect(parseList(" vigilance, Trample,,trample ")).toEqual(["vigilance", "Trample"])
  })

  it("restores saved trackers and drops malformed entries", () => {
    expect(
      parseSavedTrackers({
        counters: [{ id: "a", label: "Lands", value: 250, shared: true }, { id: "x" }, 4],
        effects: [
          { id: "e", name: "Anthem", power: 1, toughness: 1 },
          { id: "f", name: "Bad", power: "1", toughness: 1 },
        ],
        shareEffects: "yes",
      }),
    ).toEqual({
      counters: [{ id: "a", label: "Lands", value: 100, shared: true }],
      effects: [{ id: "e", name: "Anthem", power: 1, toughness: 1, conditions: [], keywords: [] }],
      shareEffects: false,
    })
    expect(parseSavedTrackers(null)).toEqual({ counters: [], effects: [], shareEffects: false })
  })
})
