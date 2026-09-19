import { api } from "./api"

export interface CardImageUris {
  small?: string
  normal?: string
  art_crop?: string
}

export interface CardSummary {
  id: string
  oracle_id: string
  name: string
  mana_cost: string | null
  type_line: string
  color_identity: string[]
  image_uris: CardImageUris
  can_be_commander: boolean
  commander_pairing: string | null
}

export type SelectedCard = CardSummary & { catalog_id: string | null }

export function selectCatalogCard(card: CardSummary): SelectedCard {
  return { ...card, catalog_id: card.id }
}

export function cardSnapshot(
  catalogId: string | null | undefined,
  name: string | null | undefined,
  colorIdentity: string[] = [],
): SelectedCard | null {
  if (!name) return null
  return {
    id: catalogId ?? `snapshot:${name}`,
    catalog_id: catalogId ?? null,
    oracle_id: "",
    name,
    mana_cost: null,
    type_line: catalogId ? "Stored card snapshot" : "Card not found in catalog",
    color_identity: colorIdentity,
    image_uris: {},
    can_be_commander: true,
    commander_pairing: null,
  }
}

const colorOrder = ["W", "U", "B", "R", "G"]

export function combinedColorIdentity(cards: Array<SelectedCard | null>) {
  const colors = new Set(cards.flatMap((card) => card?.color_identity ?? []))
  return colorOrder.filter((color) => colors.has(color)).join("")
}

export interface CatalogStatus {
  status: "never" | "running" | "succeeded" | "failed"
  last_started_at: string | null
  last_finished_at: string | null
  card_count: number
  scryfall_updated_at: string | null
  last_error: string | null
}

export async function searchCards(query: string, commanderOnly = false, limit = 20) {
  const params = new URLSearchParams({ q: query, limit: String(limit) })
  if (commanderOnly) params.set("commander", "true")
  return api<{ data: CardSummary[] }>(`/api/cards?${params}`).then((response) => response.data)
}

export function fetchCatalogStatus() {
  return api<{ data: CatalogStatus }>("/api/catalog").then((response) => response.data)
}

export function triggerCatalogSync() {
  return api<{ data: { status: "started" | "already_running" } }>("/api/catalog/sync", {
    method: "POST",
  }).then((response) => response.data)
}
