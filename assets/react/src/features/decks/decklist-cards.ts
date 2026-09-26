import { queryOptions } from "@tanstack/react-query"
import { api, ApiError } from "@/lib/api"
import type { CardImageUris } from "@/lib/cards"
import type { DeckSummary } from "./decks"

/** One entry of a linked deck list: commander zone or main deck, never maybe/sideboards. */
export interface DecklistCard {
  name: string
  quantity: number
  zone: "commander" | "mainboard"
  /** The exact Scryfall printing the list names, when the service records one. */
  printing_id: string | null
  /** Catalog details; null when the local catalog does not know the name. */
  card_id: string | null
  type_line: string | null
  mana_cost: string | null
  cmc: number | null
  game_changer: boolean
  /** Cached images of the list's printing, or the catalog's preferred one. */
  image_uris: CardImageUris
}

/** `GET /api/decks/:id/decklist`: the card list behind a deck's Moxfield, Archidekt or
 * ManaVault link, fetched live (and briefly cached) by the server. */
export interface DecklistCards {
  source: "moxfield" | "archidekt" | "manavault"
  url: string
  name: string
  fetched_at: string
  cards: DecklistCard[]
}

const LISTED_SOURCES = new Set(["moxfield", "archidekt", "manavault"])

/** The deck links to a service whose card list the server can read. */
export function hasDecklistCards(deck: Pick<DeckSummary, "decklist_url" | "decklist_source">) {
  return !!deck.decklist_url && LISTED_SOURCES.has(deck.decklist_source ?? "")
}

/** A game lasts about an hour and lists rarely change mid-game, so keep one for 30 minutes.
 * A missing or private list (404) is an answer, not a failure worth retrying. */
export function decklistCardsQuery(deckId: number) {
  return queryOptions({
    queryKey: ["decks", deckId, "decklist"],
    queryFn: () =>
      api<{ data: DecklistCards }>(`/api/decks/${deckId}/decklist`).then((body) => body.data),
    staleTime: 30 * 60 * 1000,
    retry: (failures, error) =>
      !(error instanceof ApiError && error.status === 404) && failures < 2,
  })
}

/** Total cards, counting quantities (a 100-card deck reads 100). */
export function decklistSize(cards: DecklistCard[]) {
  return cards.reduce((total, card) => total + card.quantity, 0)
}

export const DECKLIST_GROUPS = [
  "Commander",
  "Creatures",
  "Planeswalkers",
  "Battles",
  "Instants",
  "Sorceries",
  "Artifacts",
  "Enchantments",
  "Lands",
  "Other",
] as const

export type DecklistGroup = (typeof DECKLIST_GROUPS)[number]

/** Deck-builder style section from the front face's type: an artifact creature is a creature,
 * an artifact land a land. Unknown cards (not in the local catalog) land in Other. */
export function decklistGroup(card: Pick<DecklistCard, "zone" | "type_line">): DecklistGroup {
  if (card.zone === "commander") return "Commander"
  const type = card.type_line?.split(" // ")[0] ?? ""
  if (type.includes("Creature")) return "Creatures"
  if (type.includes("Land")) return "Lands"
  if (type.includes("Planeswalker")) return "Planeswalkers"
  if (type.includes("Battle")) return "Battles"
  if (type.includes("Instant")) return "Instants"
  if (type.includes("Sorcery")) return "Sorceries"
  if (type.includes("Artifact")) return "Artifacts"
  if (type.includes("Enchantment")) return "Enchantments"
  return "Other"
}

/** Non-empty sections in deck-builder order, cards by name within each. */
export function groupDecklist(cards: DecklistCard[]) {
  return DECKLIST_GROUPS.map((group) => ({
    group,
    cards: cards
      .filter((card) => decklistGroup(card) === group)
      .sort((a, b) => a.name.localeCompare(b.name)),
  })).filter((section) => section.cards.length > 0)
}
