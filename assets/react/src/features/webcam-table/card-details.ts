import { useQuery } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { CardImageUris } from "@/lib/cards"

/** `GET /api/card-printings/:id/details`: one exact printing, fetched from Scryfall by the
 * server because the catalog only keeps one printing per card. */
export interface PrintingDetails {
  id: string
  oracle_id: string
  name: string
  set_code: string
  set_name: string | null
  collector_number: string
  lang: string
  image_uris: CardImageUris
  mana_cost: string | null
  type_line: string
  oracle_text: string | null
  flavor_text: string | null
  power: string | null
  toughness: string | null
  loyalty: string | null
  layout: string
  rarity: string | null
  released_at: string | null
  scryfall_uri: string | null
}

export function getPrintingDetails(id: string) {
  return api<{ data: PrintingDetails }>(
    `/api/card-printings/${encodeURIComponent(id)}/details`,
  ).then((body) => body.data)
}

/** Details for one printing; a printing never changes, so the answer is kept for the table. */
export function usePrintingDetails(id: string | null) {
  return useQuery({
    queryKey: ["card-printings", id, "details"],
    queryFn: () => getPrintingDetails(id as string),
    enabled: id !== null,
    staleTime: Infinity,
  })
}

/** "The Hobbit Eternal · #104", falling back to the set code when the name is unknown. */
export function printingCaption(card: {
  set_name?: string | null
  set: string
  collector_number?: string | null
}) {
  const set = card.set_name ?? card.set.toUpperCase()
  return card.collector_number ? `${set} · #${card.collector_number}` : set
}
