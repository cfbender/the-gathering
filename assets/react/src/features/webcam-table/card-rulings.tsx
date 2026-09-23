import { useQuery } from "@tanstack/react-query"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { api } from "@/lib/api"
import type { IdentifiedCard } from "./use-webcam-room"

interface Ruling {
  source: string
  published_at: string
  comment: string
}

export function CardRulings({ card, onClose }: { card: IdentifiedCard; onClose: () => void }) {
  const rulings = useQuery({
    queryKey: ["card-printings", card.id, "rulings"],
    queryFn: () =>
      api<{ data: Ruling[] }>(`/api/card-printings/${encodeURIComponent(card.id)}/rulings`).then(
        (body) => body.data,
      ),
    staleTime: 60 * 60 * 1000,
  })
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="sm:max-w-xl border-white/15 bg-neutral-950 text-white">
        <DialogHeader>
          <DialogTitle>{card.name} · Rulings</DialogTitle>
          <DialogClose onClose={onClose} />
        </DialogHeader>
        <div className="p-5 text-sm">
          {rulings.isPending && <p role="status">Loading Scryfall rulings…</p>}
          {rulings.isError && (
            <div role="alert">
              <p>Rulings are unavailable right now.</p>
              <button
                type="button"
                className="btn btn-sm mt-2"
                onClick={() => void rulings.refetch()}
              >
                Retry
              </button>
            </div>
          )}
          {rulings.data?.length === 0 && <p>No rulings published on Scryfall for this card.</p>}
          <ol className="grid gap-4">
            {rulings.data?.map((ruling, index) => (
              <li key={index}>
                <p className="mb-1 text-xs text-white/50">
                  <time>{ruling.published_at}</time> · {ruling.source}
                </p>
                <p className="whitespace-pre-line">{ruling.comment}</p>
              </li>
            ))}
          </ol>
        </div>
      </DialogContent>
    </Dialog>
  )
}
