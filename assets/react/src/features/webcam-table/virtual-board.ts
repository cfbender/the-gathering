import {
  createPlaytestState,
  type PlaytestCard,
  type PlaytestState,
  type PublicPlaytestCard,
  type PublicPlaytestSnapshot,
} from "@cfbender/playtest"
import type { DecklistCards } from "@/features/decks/decklist-cards"

/** The server's card image cache; boards from other seats may only point here. */
const CARD_IMAGE_PATH = "/api/card-images?"
const MAX_ZONE_CARDS = 250
const MAX_TEXT = 300

/** A new game from a seat's linked deck list: the commander zone, and the rest shuffled into
 * the library with an opening hand drawn. `deckCardId` carries the printing (or the catalog's
 * card) so viewers can open the card's details. */
export function dealDecklist(list: DecklistCards): PlaytestState {
  const library: PlaytestCard[] = []
  const command: PlaytestCard[] = []
  list.cards.forEach((card, index) => {
    for (let copy = 0; copy < card.quantity; copy += 1) {
      const target = card.zone === "commander" ? command : library
      target.push({
        id: `${index}:${copy}`,
        deckCardId: card.printing_id ?? card.card_id ?? card.name,
        name: card.name,
        imageUrl: card.image_uris.normal ?? card.image_uris.small ?? null,
        manaCost: card.mana_cost,
        typeLine: card.type_line,
      })
    }
  })
  return createPlaytestState(library, command)
}

type Fields = Record<string, unknown>

function isRecord(value: unknown): value is Fields {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function text(value: unknown): value is string {
  return typeof value === "string" && value.length <= MAX_TEXT
}

function optionalText(value: unknown) {
  return text(value) ? value : null
}

function count(value: unknown): value is number {
  return Number.isInteger(value) && (value as number) >= 0 && (value as number) <= 9999
}

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value)
}

function parseCard(value: unknown): PublicPlaytestCard | null {
  if (!isRecord(value) || !text(value.id) || !value.id) return null
  if (value.hidden === true) return { hidden: true, id: value.id }
  if (!text(value.name) || !text(value.deckCardId)) return null
  const image = optionalText(value.imageUrl)
  return {
    id: value.id,
    deckCardId: value.deckCardId,
    name: value.name,
    imageUrl: image?.startsWith(CARD_IMAGE_PATH) ? image : null,
    manaCost: optionalText(value.manaCost),
    setLabel: optionalText(value.setLabel),
    typeLine: optionalText(value.typeLine),
  }
}

function parseZone(value: unknown) {
  if (!Array.isArray(value) || value.length > MAX_ZONE_CARDS) return null
  const cards = value.map(parseCard)
  return cards.every((card) => card !== null) ? cards : null
}

function parseRecord<T>(value: unknown, parse: (entry: unknown) => T | null) {
  if (!isRecord(value)) return null
  const parsed: Record<string, T> = {}
  for (const [key, entry] of Object.entries(value)) {
    const result = parse(entry)
    if (result === null) return null
    parsed[key] = result
  }
  return parsed
}

function parseStatus(value: unknown) {
  if (!isRecord(value)) return null
  const { markers, minusOneCounters, plusOneCounters } = value
  if (![markers, minusOneCounters, plusOneCounters].every(count)) return null
  return {
    markers: markers as number,
    minusOneCounters: minusOneCounters as number,
    plusOneCounters: plusOneCounters as number,
    faceDown: value.faceDown === true,
    power: text(value.power) ? value.power : undefined,
    toughness: text(value.toughness) ? value.toughness : undefined,
  }
}

/** Another seat's board as published to the table, or null for anything malformed. Peers are
 * untrusted: only known fields survive, and card images must come from the server's cache. */
export function parseBoard(value: unknown): PublicPlaytestSnapshot | null {
  if (!isRecord(value)) return null
  const [battlefield, command, exile, graveyard] = [
    value.battlefield,
    value.command,
    value.exile,
    value.graveyard,
  ].map(parseZone)
  const positions = parseRecord(value.battlefieldCardPositions, (position) =>
    isRecord(position) && finite(position.x) && finite(position.y)
      ? { x: position.x, y: position.y }
      : null,
  )
  const statuses = parseRecord(value.cardStatuses, parseStatus)
  const counters = value.playerCounters
  const tapped = value.tappedCardIds
  if (
    !battlefield ||
    !command ||
    !exile ||
    !graveyard ||
    !positions ||
    !statuses ||
    !count(value.handCount) ||
    !count(value.libraryCount) ||
    !Array.isArray(tapped) ||
    !tapped.every(text) ||
    !isRecord(counters)
  )
    return null
  return {
    battlefield,
    battlefieldCardPositions: positions,
    cardStatuses: statuses,
    command,
    exile,
    graveyard,
    handCount: value.handCount,
    libraryCount: value.libraryCount,
    lifeTotal: finite(value.lifeTotal) ? value.lifeTotal : 0,
    mulligans: count(value.mulligans) ? value.mulligans : 0,
    openingHand: value.openingHand === true,
    playerCounters: {
      energy: count(counters.energy) ? counters.energy : 0,
      experience: count(counters.experience) ? counters.experience : 0,
      poison: count(counters.poison) ? counters.poison : 0,
    },
    tappedCardIds: tapped,
    turn: count(value.turn) ? value.turn : 1,
  }
}
