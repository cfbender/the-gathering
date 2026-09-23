import { Trash2, Undo2, X } from "lucide-react"
import { useEffect } from "react"
import { CardImage } from "@/components/card-image"
import { ManaCost, ManaSymbol, parseManaCost } from "@/components/mana-symbols"
import { cn } from "@/lib/cn"
import { printingCaption, usePrintingDetails, type PrintingDetails } from "./card-details"
import type { IdentifiedCard } from "./use-webcam-room"

interface Props {
  card: IdentifiedCard
  /** Whose board the card is on; omitted for a card previewed from a search. */
  ownerName?: string
  /** Offered while the capture that produced this card is still around: reopens the picker
   * for the same crop so the entry can be replaced. */
  onWrongCard?: () => void
  /** Offered for a card on a board's list. */
  onRemove?: () => void
  onClose: () => void
}

function stats(details: PrintingDetails) {
  if (details.power !== null && details.toughness !== null)
    return `${details.power}/${details.toughness}`
  if (details.loyalty !== null) return `Loyalty ${details.loyalty}`
  return null
}

/** Rules text with each `\n` as its own paragraph, `//` as the face divider, and `{T}`-style
 * tokens drawn as symbols. */
function OracleText({ text }: { text: string }) {
  return (
    <div className="grid gap-1.5">
      {text
        .split("\n")
        .map((line, index) =>
          line === "//" ? (
            <hr key={index} className="border-white/15" />
          ) : (
            <p key={index}>
              {parseManaCost(line).map((part, partIndex) =>
                part.kind === "symbol" ? (
                  <ManaSymbol key={partIndex} symbol={part.token} className="h-[1em] w-[1em]" />
                ) : (
                  <span key={partIndex}>{part.text}</span>
                ),
              )}
            </p>
          ),
        )}
    </div>
  )
}

/** Card and rules text for one printing, shown over the board after a click identifies a card
 * or when an entry in a list is opened. Escape or the backdrop closes it. */
export function CardPreview({ card, ownerName, onWrongCard, onRemove, onClose }: Props) {
  const details = usePrintingDetails(card.id)

  useEffect(() => {
    function close(event: KeyboardEvent) {
      if (event.key === "Escape") onClose()
    }
    window.addEventListener("keydown", close)
    return () => window.removeEventListener("keydown", close)
  }, [onClose])

  const data = details.data
  const caption = data
    ? printingCaption({ ...data, set: data.set_code })
    : printingCaption({ set: card.set, collector_number: card.collector_number })

  return (
    <div
      className="absolute inset-0 z-20 flex items-center justify-center bg-black/65 p-4 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-label={`${card.name} details`}
      onClick={onClose}
    >
      <div
        className="flex max-h-full max-w-full flex-col items-center gap-3"
        onClick={(event) => event.stopPropagation()}
      >
        {(onWrongCard || onRemove) && (
          <div className="flex items-center gap-2">
            {onWrongCard && (
              <button
                type="button"
                className="btn btn-warning btn-sm h-9 rounded-full px-4 text-xs font-bold"
                onClick={onWrongCard}
              >
                <Undo2 className="size-3.5" />
                <span className="grid text-left leading-tight">
                  Wrong card?
                  <span className="text-[0.6rem] font-medium opacity-80">Pick the right one</span>
                </span>
              </button>
            )}
            {onRemove && (
              <button
                type="button"
                className="btn btn-sm h-9 rounded-full border-white/20 bg-black/70 px-4 text-xs text-white hover:bg-black/90"
                onClick={onRemove}
              >
                <Trash2 className="size-3.5" /> Remove from board
              </button>
            )}
          </div>
        )}

        <div className="flex min-h-0 items-start gap-3">
          <aside
            className="hidden w-72 shrink-0 rounded-xl border border-white/10 bg-black/80 p-4 text-sm text-white shadow-2xl md:block"
            aria-label="Rules text"
          >
            {data?.mana_cost && <ManaCost cost={data.mana_cost} className="text-base" />}
            <h2 className="mt-1 text-base font-bold">{card.name}</h2>
            <p className="text-white/70">
              {data ? data.type_line : "Loading…"}
              {data && stats(data) && <> · {stats(data)}</>}
            </p>
            {data?.oracle_text && (
              <div className="mt-3 text-[0.8rem] leading-snug">
                <OracleText text={data.oracle_text} />
              </div>
            )}
            {details.isError && (
              <p className="mt-3 text-xs text-white/50">
                Rules text is unavailable right now (Scryfall could not be reached).
              </p>
            )}
          </aside>

          <figure className="relative flex min-h-0 flex-col items-center rounded-2xl border border-white/10 bg-black/80 p-3 shadow-2xl">
            <button
              type="button"
              className="absolute top-4 right-4 z-10 grid size-7 place-items-center rounded-full bg-black/70 text-white/80 hover:bg-black hover:text-white"
              onClick={onClose}
              aria-label="Close card details"
            >
              <X className="size-4" />
            </button>
            <div className={cn("w-[min(22rem,60vw)]", !data && "animate-pulse")}>
              <CardImage
                imageUris={data?.image_uris ?? {}}
                name={card.name}
                variant="card"
                className="w-full"
              />
            </div>
            <figcaption className="mt-2 text-center text-xs text-white/70">
              {caption}
              {ownerName && <span className="text-white/45"> · {ownerName}’s board</span>}
            </figcaption>
          </figure>
        </div>
      </div>
    </div>
  )
}
