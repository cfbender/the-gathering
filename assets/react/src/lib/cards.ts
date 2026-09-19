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
