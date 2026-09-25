import { queryOptions, useQuery, type QueryClient } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { CardImageUris } from "@/lib/cards"

/** `GET /api/card-printings/:id/details`: one exact printing, fetched from Scryfall by the
 * server because the catalog only keeps one printing per card. */
export interface PrintingDetails {
  id: string
  oracle_id: string
  name: string
  game_changer?: boolean
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
  prices: { usd: string | null; usd_foil: string | null; usd_etched: string | null }
}

export function getPrintingDetails(id: string) {
  return api<{ data: PrintingDetails }>(
    `/api/card-printings/${encodeURIComponent(id)}/details`,
  ).then((body) => body.data)
}

/** Prices change, so refresh printing details after one hour. */
function printingDetailsQuery(id: string) {
  return queryOptions({
    queryKey: ["card-printings", id, "details"],
    queryFn: () => getPrintingDetails(id),
    staleTime: 60 * 60 * 1000,
  })
}

export function usePrintingDetails(id: string | null) {
  return useQuery({ ...printingDetailsQuery(id as string), enabled: id !== null })
}

/** Starts a low-priority download so a later `<img>` with the same URL renders from cache. */
export function preloadImage(src: string | undefined) {
  if (!src) return
  const image = new Image()
  image.fetchPriority = "low"
  image.decoding = "async"
  image.src = src
}

/** Warms the details cache and both card images (tray thumb and preview) for printings named
 * at the table, so opening one does not wait on Scryfall. Lookups run one at a time, in the
 * order given, so a mid-game join's backlog does not crowd out other seats' fresh clicks on
 * the server's shared Scryfall limit. A failure is left for the query to retry when something
 * actually shows the card. */
export async function prefetchPrintings(queryClient: QueryClient, ids: Iterable<string>) {
  for (const id of ids) {
    try {
      const details = await queryClient.fetchQuery(printingDetailsQuery(id))
      preloadImage(details.image_uris.small)
      preloadImage(details.image_uris.normal)
    } catch {
      // The tray or preview refetches an errored query when it mounts.
    }
  }
}

/** Details cached by a browser or proxy before prices shipped have no `prices` field. */
export function printingPrices(prices: PrintingDetails["prices"] | null | undefined) {
  if (!prices) return "—"
  return (
    [
      prices.usd && `$${prices.usd}`,
      prices.usd_foil && `Foil $${prices.usd_foil}`,
      prices.usd_etched && `Etched $${prices.usd_etched}`,
    ]
      .filter(Boolean)
      .join(" · ") || "—"
  )
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
