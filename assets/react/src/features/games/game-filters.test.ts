import { describe, expect, it } from "vite-plus/test"
import {
  detailedScope,
  colorChoice,
  colorChoicePatch,
  gameFilterChips,
  gamesLink,
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
})

describe("clock filters", () => {
  it("keeps zero-based weekdays and hours within range", () => {
    expect(parseGamesSearch({ weekday: "0", hour: 0 })).toEqual({ weekday: 0, hour: 0 })
    expect(parseGamesSearch({ weekday: 6, hour: "23" })).toEqual({ weekday: 6, hour: 23 })
    expect(parseGamesSearch({ weekday: 7, hour: 24 })).toEqual({})
    expect(parseGamesSearch({ weekday: -1, hour: "1.5" })).toEqual({})
  })
})

describe("gamesLink", () => {
  it("targets the games list with the scope's range start and drops unset filters", () => {
    expect(gamesLink({ date_from: "2026-03-25" }, { player_id: undefined, colors: "BG" })).toEqual({
      to: "/games",
      search: { date_from: "2026-03-25", colors: "BG" },
    })
    expect(gamesLink({}, { winner_seat: 2 })).toEqual({ to: "/games", search: { winner_seat: 2 } })
  })

  it("lets a link override the range, for example a single calendar day", () => {
    expect(
      gamesLink({ date_from: "2026-01-01" }, { date_from: "2026-05-04", date_to: "2026-05-04" })
        .search,
    ).toEqual({ date_from: "2026-05-04", date_to: "2026-05-04" })
  })
})

describe("detailedScope", () => {
  it("starts no earlier than the detailed-stats cutoff", () => {
    expect(detailedScope({ player_id: 3 }, "2026-02-01")).toEqual({
      player_id: 3,
      date_from: "2026-02-01",
    })
    expect(detailedScope({ date_from: "2026-01-01" }, "2026-02-01").date_from).toBe("2026-02-01")
    expect(detailedScope({ date_from: "2026-03-01" }, "2026-02-01").date_from).toBe("2026-03-01")
    expect(detailedScope({ date_from: "2026-03-01" }, null).date_from).toBe("2026-03-01")
  })
})

describe("gameFilterChips", () => {
  it("names every active filter with player, color, condition, and date labels", () => {
    const names: Record<number, string> = { 1: "Alice", 2: "Bob" }
    expect(
      gameFilterChips(
        {
          player_id: 1,
          player_result: "loss",
          opponent_id: 2,
          winner_colors: "BG",
          color: "U",
          win_condition: "mill",
          winner_seat: 3,
          min_turns: 5,
          date_from: "2025-09-25",
          weekday: 2,
          hour: 23,
          commander: "Meren",
          page: 2,
        },
        { player: (id) => names[id], winCondition: (condition) => `<${condition}>` },
      ).map((chip) => chip.label),
    ).toEqual([
      "Player: Alice",
      "Alice lost",
      "With Bob",
      "Commander: Meren",
      "Won with Golgari",
      "Includes Blue",
      "Ended by <mill>",
      "Won from seat 3",
      "At least 5 turns",
      `From ${new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(2025, 8, 25))}`,
      "On Tuesdays",
      "Played 11pm–12am",
    ])
  })
})

describe("color choice", () => {
  it("reads any of the four color keys into one control value", () => {
    expect(colorChoice({})).toEqual({ value: "", won: false })
    expect(colorChoice({ colors: "WU" })).toEqual({ value: "WU", won: false })
    expect(colorChoice({ winner_colors: "WU" })).toEqual({ value: "WU", won: true })
    expect(colorChoice({ color: "B" })).toEqual({ value: "has:B", won: false })
    expect(colorChoice({ winner_color: "B" })).toEqual({ value: "has:B", won: true })
  })

  it("writes one key and clears the others, so toggling Won swaps played for won", () => {
    const search = patchGamesSearch(
      { colors: "WU", date_from: "2025-09-25" },
      colorChoicePatch({ value: "WU", won: true }),
    )
    expect(search).toEqual({ winner_colors: "WU", date_from: "2025-09-25" })
    expect(patchGamesSearch(search, colorChoicePatch({ value: "has:G", won: false }))).toEqual({
      color: "G",
      date_from: "2025-09-25",
    })
    expect(patchGamesSearch(search, colorChoicePatch({ value: "", won: true }))).toEqual({
      date_from: "2025-09-25",
    })
  })
})
