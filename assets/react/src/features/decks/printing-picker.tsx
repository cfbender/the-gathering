import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query"
import { useId, useState } from "react"
import { GameChangerBadge } from "@/components/game-changer-badge"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ApiError } from "@/lib/api"
import type { SelectedCard } from "@/lib/cards"
import { cn } from "@/lib/cn"
import { getPrinting, getPrintings, printingLabel, type CardPrinting } from "./printings"

export function PrintingPicker({
  card,
  label,
  onChange,
}: {
  card: SelectedCard
  label: string
  onChange: (id: string | null) => void
}) {
  const [open, setOpen] = useState(false)
  const descriptionId = useId()
  const queryClient = useQueryClient()
  const selected = useQuery({
    queryKey: ["card-printings", card.printing_id],
    queryFn: () => getPrinting(card.printing_id!),
    enabled: Boolean(card.printing_id),
    staleTime: Infinity,
    retry: false,
  })
  const printings = useInfiniteQuery({
    queryKey: ["card-printings", "options", card.catalog_id, card.name],
    queryFn: ({ pageParam }) => getPrintings(card, pageParam),
    initialPageParam: 1,
    getNextPageParam: (last, pages) => (last.has_more ? pages.length + 1 : undefined),
    enabled: open,
    staleTime: 60 * 60 * 1000,
    retry: false,
  })
  const options = printings.data?.pages.flatMap((page) => page.data) ?? []
  const preview = selected.data?.image_uris.art_crop ?? card.image_uris.art_crop

  function choose(printing: CardPrinting | null) {
    if (printing) queryClient.setQueryData(["card-printings", printing.id], printing)
    onChange(printing?.id ?? null)
    setOpen(false)
  }

  return (
    <div className="mt-2">
      <button
        type="button"
        className="border-base-300 hover:bg-base-300/40 flex w-full items-center gap-3 rounded-lg border p-2 text-left"
        aria-label={`${label} printing: ${selected.data ? printingLabel(selected.data) : card.printing_id ? "Selected printing" : "Catalog default"}`}
        onClick={() => setOpen(true)}
      >
        {preview && (
          <img
            src={preview}
            alt=""
            loading="lazy"
            decoding="async"
            className="h-12 w-16 shrink-0 rounded-md object-cover"
          />
        )}
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-medium">Choose printing</span>
          <span className="text-base-content/60 block text-xs">
            {selected.data
              ? printingLabel(selected.data)
              : card.printing_id
                ? "Selected printing"
                : "Catalog default"}
          </span>
        </span>
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-3xl" describedBy={descriptionId}>
          <DialogHeader>
            <div>
              <DialogTitle>{label} printing</DialogTitle>
              <p id={descriptionId} className="text-base-content/60 mt-1 text-sm">
                {card.name} · Choose the card you own, then save your deck. This only changes
                artwork.
              </p>
            </div>
            <DialogClose onClose={() => setOpen(false)} />
          </DialogHeader>
          <div className="space-y-4 p-4 sm:p-5">
            <button type="button" className="btn btn-outline btn-sm" onClick={() => choose(null)}>
              Use catalog default
            </button>
            {printings.isPending && <p role="status">Loading printings from Scryfall…</p>}
            {printings.isError && (
              <div role="alert" className="space-y-2">
                <p>
                  {printings.error instanceof ApiError && printings.error.status === 404
                    ? "This card is not in the catalog yet. Select a catalog card to choose its printing."
                    : "Printings could not be loaded. Your selection is unchanged. Try again in a moment."}
                </p>
                <button
                  type="button"
                  className="btn btn-outline btn-sm"
                  onClick={() =>
                    void (printings.isFetchNextPageError
                      ? printings.fetchNextPage()
                      : printings.refetch())
                  }
                >
                  Try again
                </button>
              </div>
            )}
            {printings.isSuccess && options.length === 0 && <p>No paper printings found.</p>}
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
              {options.map((printing) => (
                <button
                  key={printing.id}
                  type="button"
                  aria-pressed={card.printing_id === printing.id}
                  className={cn(
                    "border-base-300 hover:border-primary flex flex-col gap-2 rounded-xl border-2 p-2 text-left",
                    card.printing_id === printing.id && "border-primary bg-primary/10",
                  )}
                  onClick={() => choose(printing)}
                >
                  {printing.image_uris.normal && (
                    <img
                      src={printing.image_uris.normal}
                      alt=""
                      loading="lazy"
                      decoding="async"
                      className="aspect-[5/7] w-full rounded-lg object-contain"
                    />
                  )}
                  <span className="text-sm font-medium">{printingLabel(printing)}</span>
                  <GameChangerBadge gameChanger={printing.game_changer} />
                  {card.printing_id === printing.id && (
                    <span className="text-primary text-xs font-bold">Selected</span>
                  )}
                </button>
              ))}
            </div>
            {printings.hasNextPage && (
              <button
                type="button"
                className="btn btn-outline w-full"
                disabled={printings.isFetching}
                onClick={() => void printings.fetchNextPage()}
              >
                {printings.isFetchingNextPage ? "Loading…" : "More printings"}
              </button>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
