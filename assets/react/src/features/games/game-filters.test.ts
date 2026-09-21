import { describe, expect, it } from "vite-plus/test"
import {
  countActiveGameFilters,
  parseGamesSearch,
  patchGamesSearch,
  toGameFilters,
} from "./game-filters"

describe("parseGamesSearch", () => {
  it("keeps known filters, trims them, and drops empty or unknown keys", () => {
    expect(
      parseGamesSearch({
        player_id: "9",
        commander: " Kaalia of the Vast ",
        winner_id: "",
        min_turns: 8,
        bogus: "x",
      }),
    ).toEqual({ player_id: 9, commander: "Kaalia of the Vast", min_turns: 8 })
  })

  it("only keeps a page beyond the first", () => {
    expect(parseGamesSearch({ page: "3" })).toEqual({ page: 3 })
    expect(parseGamesSearch({ page: 1 })).toEqual({})
    expect(parseGamesSearch({ page: "abc" })).toEqual({})
    expect(parseGamesSearch({ page: 2.5 })).toEqual({})
  })

  it("ignores array and object values", () => {
    expect(parseGamesSearch({ commander: ["a", "b"], player_id: { id: 1 } })).toEqual({})
  })

  it("drops numeric filters that are not positive integers", () => {
    expect(
      parseGamesSearch({ player_id: "abc", min_turns: "0", max_turns: "-3", player_count: "4.5" }),
    ).toEqual({})
  })
})

describe("toGameFilters", () => {
  it("fills every filter with an empty string when unset", () => {
    const filters = toGameFilters({ commander: "Krenko", winner_id: 3, page: 4 })
    expect(filters.commander).toBe("Krenko")
    expect(filters.winner_id).toBe("3")
    expect(filters.player_id).toBe("")
    expect(filters.max_duration).toBe("")
    expect("page" in filters).toBe(false)
  })
})

describe("patchGamesSearch", () => {
  it("applies the change, removes cleared filters, and returns to page one", () => {
    expect(
      patchGamesSearch({ player_id: 9, commander: "Krenko", page: 3 }, { commander: "" }),
    ).toEqual({ player_id: 9 })
    expect(patchGamesSearch({ player_id: 9 }, { min_turns: "12" })).toEqual({
      player_id: 9,
      min_turns: 12,
    })
  })

  it("counts active filters without the page", () => {
    expect(countActiveGameFilters({ player_id: 1, commander: "x", page: 2 })).toBe(2)
    expect(countActiveGameFilters({})).toBe(0)
  })
})
