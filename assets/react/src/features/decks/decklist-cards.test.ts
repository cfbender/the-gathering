import { expect, it } from "vite-plus/test"
import {
  decklistGroup,
  decklistSize,
  groupDecklist,
  hasDecklistCards,
  type DecklistCard,
} from "./decklist-cards"

function card(name: string, type_line: string | null, overrides: Partial<DecklistCard> = {}) {
  return {
    name,
    quantity: 1,
    zone: "mainboard",
    printing_id: null,
    card_id: null,
    type_line,
    mana_cost: null,
    cmc: null,
    game_changer: false,
    image_uris: {},
    ...overrides,
  } satisfies DecklistCard
}

it("groups by the front face's primary type", () => {
  expect(decklistGroup(card("Atraxa", "Legendary Creature", { zone: "commander" }))).toBe(
    "Commander",
  )
  expect(decklistGroup(card("Solemn", "Artifact Creature — Golem"))).toBe("Creatures")
  expect(decklistGroup(card("Dryad Arbor", "Land Creature — Forest Dryad"))).toBe("Creatures")
  expect(decklistGroup(card("Seat of the Synod", "Artifact Land"))).toBe("Lands")
  expect(decklistGroup(card("Urza's Saga", "Enchantment Land — Urza's Saga"))).toBe("Lands")
  expect(decklistGroup(card("Sol Ring", "Artifact"))).toBe("Artifacts")
  expect(decklistGroup(card("Invasion of Ikoria", "Battle — Siege // Creature"))).toBe("Battles")
  expect(
    decklistGroup(card("Brazen Borrower // Petty Theft", "Creature — Faerie // Instant")),
  ).toBe("Creatures")
  expect(decklistGroup(card("Unknown", null))).toBe("Other")
})

it("orders sections like a deck builder and sorts names within them", () => {
  const sections = groupDecklist([
    card("Sol Ring", "Artifact"),
    card("Island", "Basic Land — Island", { quantity: 30 }),
    card("Arcane Signet", "Artifact"),
    card("Kenrith", "Legendary Creature", { zone: "commander" }),
  ])
  expect(sections.map(({ group, cards }) => [group, cards.map((c) => c.name)])).toEqual([
    ["Commander", ["Kenrith"]],
    ["Artifacts", ["Arcane Signet", "Sol Ring"]],
    ["Lands", ["Island"]],
  ])
  expect(decklistSize(sections.flatMap((section) => section.cards))).toBe(33)
})

it("only reads lists from supported sites", () => {
  expect(
    hasDecklistCards({ decklist_url: "https://moxfield.com/decks/a", decklist_source: "moxfield" }),
  ).toBe(true)
  expect(hasDecklistCards({ decklist_url: "https://example.com", decklist_source: "other" })).toBe(
    false,
  )
  expect(hasDecklistCards({ decklist_url: null, decklist_source: null })).toBe(false)
})
