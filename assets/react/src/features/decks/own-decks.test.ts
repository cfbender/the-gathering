import { describe, expect, it } from "vite-plus/test"
import type { DeckSummary } from "@/features/decks/decks"
import { ownDecks, parseDeckScope, resolveDeckScope } from "./own-decks"

const deck = (id: number, user_id: number | null): DeckSummary => ({
  id,
  player_id: id,
  name: `Deck ${id}`,
  commander_card_id: null,
  commander_name: "Krenko",
  partner_card_id: null,
  partner_name: null,
  color_identity: "R",
  decklist_url: null,
  decklist_source: null,
  archived_at: null,
  skip_count: 0,
  included_for_play: true,
  commander_art_crop_url: null,
  partner_art_crop_url: null,
  player: { id, name: `P${id}`, avatar_url: null, user_id, archived_at: null },
})

describe("ownDecks", () => {
  it("keeps only decks whose player is linked to the viewer", () => {
    const decks = [deck(1, 7), deck(2, null), deck(3, 8)]
    expect(ownDecks(decks, 7).map((d) => d.id)).toEqual([1])
  })

  it("matches nothing while the viewer is unknown, even for unclaimed players", () => {
    expect(ownDecks([deck(2, null)], undefined)).toEqual([])
  })
})

describe("deck scope search param", () => {
  it("accepts only the two known scopes", () => {
    expect(parseDeckScope("mine")).toBe("mine")
    expect(parseDeckScope("all")).toBe("all")
    expect(parseDeckScope("MINE")).toBeUndefined()
    expect(parseDeckScope(["mine"])).toBeUndefined()
    expect(parseDeckScope(undefined)).toBeUndefined()
  })

  it("honours an explicit choice even when it would show an empty list", () => {
    expect(resolveDeckScope("mine", 0)).toBe("mine")
    expect(resolveDeckScope("all", 3)).toBe("all")
  })

  it("defaults to the viewer's decks only when they have some", () => {
    expect(resolveDeckScope(undefined, 3)).toBe("mine")
    expect(resolveDeckScope(undefined, 0)).toBe("all")
  })
})
