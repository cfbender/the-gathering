import { useQuery } from "@tanstack/react-query"
import { useEffect, useMemo, useState } from "react"
import { getPrintings, type CardPrinting } from "@/features/decks/printings"
import type { CardImageUris } from "@/lib/cards"
import type { IdentifiedCard } from "./use-webcam-room"

/** Fetch every page in API order; selection is local to the preview, never the shared tray. */
export function usePreviewPrintings(initial: IdentifiedCard) {
  const [selectedId, setSelectedId] = useState(initial.id)
  const query = useQuery({
    queryKey: ["card-printings", "preview", initial.name],
    queryFn: async () => {
      const printings: CardPrinting[] = []
      let page = 1
      let hasMore = true
      while (hasMore) {
        const result = await getPrintings({ name: initial.name, catalog_id: null }, page++)
        printings.push(...result.data)
        hasMore = result.has_more
        // The shared Scryfall search endpoint permits one request per 500 ms.
        if (hasMore) await new Promise((resolve) => setTimeout(resolve, 500))
      }
      return printings
    },
    staleTime: 60 * 60 * 1000,
  })
  const printings = useMemo<
    Array<IdentifiedCard & { image_uris?: CardImageUris; set_name?: string }>
  >(() => {
    const listed = query.data?.map((printing) => ({ ...printing, set: printing.set_code })) ?? []
    return listed.some((printing) => printing.id === initial.id) ? listed : [initial, ...listed]
  }, [initial, query.data])
  const index = Math.max(
    0,
    printings.findIndex((printing) => printing.id === selectedId),
  )
  const card = printings[index]!
  const neighbour = (offset: number) =>
    printings[(index + offset + printings.length) % printings.length]!
  const previous = neighbour(-1)
  const next = neighbour(1)

  useEffect(() => {
    for (const printing of [previous, next]) {
      if (printing.image_uris?.normal) {
        const image = new Image()
        image.fetchPriority = "low"
        image.decoding = "async"
        image.src = printing.image_uris.normal
      }
    }
  }, [previous, next])

  return {
    card,
    index,
    total: printings.length,
    previous: () => setSelectedId(previous.id),
    next: () => setSelectedId(next.id),
    isPending: query.isPending,
    isError: query.isError,
    retry: () => void query.refetch(),
  }
}
