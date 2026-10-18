import { initialSnapshot, projectPublicSnapshot } from "@cfbender/playtest"
import { expect, it } from "vite-plus/test"
import type { DecklistCard, DecklistCards } from "@/features/decks/decklist-cards"
import { dealDecklist, parseBoard } from "./virtual-board"

const IMAGE =
  "/api/card-images?url=https%3A%2F%2Fcards.scryfall.io%2Fnormal%2Ffront%2Fa%2Fb%2Fab.jpg"

function card(name: string, quantity: number, zone: DecklistCard["zone"]): DecklistCard {
  return {
    name,
    quantity,
    zone,
    printing_id: zone === "commander" ? "0e4ea6f6-4c0c-4f4a-8a9a-1c2d3e4f5a6b" : null,
    card_id: `${name}-card`,
    type_line: null,
    mana_cost: null,
    cmc: null,
    game_changer: false,
    image_uris: { normal: IMAGE },
  }
}

const list: DecklistCards = {
  source: "archidekt",
  url: "https://archidekt.com/decks/1",
  name: "Test",
  fetched_at: "2026-10-18T00:00:00Z",
  cards: [
    card("Temmet", 1, "commander"),
    card("Swamp", 30, "mainboard"),
    card("Sol Ring", 1, "mainboard"),
  ],
}

it("deals every copy, keeps the commander out of the library, and draws an opening hand", () => {
  const state = dealDecklist(list)
  expect(state.command.map((dealt) => dealt.name)).toEqual(["Temmet"])
  expect(state.command[0]!.deckCardId).toBe("0e4ea6f6-4c0c-4f4a-8a9a-1c2d3e4f5a6b")
  expect(state.hand).toHaveLength(7)
  expect(state.library).toHaveLength(31 - 7)
  const ids = [...state.hand, ...state.library].map((dealt) => dealt.id)
  expect(new Set(ids).size).toBe(31)
})

it("accepts a published board and keeps hidden cards anonymous", () => {
  const snapshot = initialSnapshot(dealDecklist(list))
  const played = snapshot.state.hand[0]!
  const board = projectPublicSnapshot({
    ...snapshot,
    state: {
      ...snapshot.state,
      hand: snapshot.state.hand.slice(1),
      battlefield: [played],
    },
    cardStatuses: {
      [played.id]: { faceDown: true, markers: 0, minusOneCounters: 0, plusOneCounters: 1 },
    },
    tappedCardIds: [played.id],
  })
  const parsed = parseBoard(JSON.parse(JSON.stringify(board)))
  expect(parsed).toMatchObject(board)
  expect(parsed!.handCount).toBe(6)
  expect(parsed!.battlefield[0]).toEqual({ hidden: true, id: expect.any(String) })
  expect(JSON.stringify(parsed)).not.toContain(played.name)
})

it("rejects malformed boards and drops card images that are not the server's", () => {
  const board = projectPublicSnapshot(initialSnapshot(dealDecklist(list)))
  const commander = board.command[0]!
  expect(parseBoard(null)).toBeNull()
  expect(parseBoard({ ...board, handCount: -1 })).toBeNull()
  expect(parseBoard({ ...board, battlefield: "Swamp" })).toBeNull()
  expect(parseBoard({ ...board, command: [{ id: "x" }] })).toBeNull()
  expect(parseBoard({ ...board, tappedCardIds: [1] })).toBeNull()

  for (const imageUrl of [
    "https://evil.example/track.png",
    "javascript:alert(1)",
    "//evil/x.jpg",
  ]) {
    const parsed = parseBoard({ ...board, command: [{ ...commander, imageUrl }] })
    expect(parsed!.command[0]).toMatchObject({ name: "Temmet", imageUrl: null })
  }
  expect(parseBoard(board)!.command[0]).toMatchObject({ imageUrl: IMAGE })
})
