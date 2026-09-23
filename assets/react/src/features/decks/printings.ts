import { api } from "@/lib/api"
import type { CardImageUris, SelectedCard } from "@/lib/cards"

export interface CardPrinting {
  id: string
  name: string
  game_changer?: boolean
  set_code: string
  set_name: string
  collector_number: string
  lang: string
  image_uris: CardImageUris
}

export function printingLabel(printing: CardPrinting) {
  const language = printing.lang === "en" ? "" : ` · ${printing.lang.toUpperCase()}`
  return `${printing.set_name} (${printing.set_code.toUpperCase()}) #${printing.collector_number}${language}`
}

export function getPrintings(card: Pick<SelectedCard, "name" | "catalog_id">, page: number) {
  const params = new URLSearchParams({ name: card.name, page: String(page) })
  if (card.catalog_id) params.set("card_id", card.catalog_id)
  return api<{ data: CardPrinting[]; has_more: boolean }>(`/api/card-printings?${params}`)
}

export function getPrinting(id: string) {
  return api<{ data: CardPrinting }>(`/api/card-printings/${id}`).then((body) => body.data)
}
