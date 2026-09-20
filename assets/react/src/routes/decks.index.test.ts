import { describe, expect, it } from "vite-plus/test"
import type { Deck } from "@/lib/games"
import { ownDecks } from "./decks.index"

const deck = (id: number, user_id: number | null): Deck => ({
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
