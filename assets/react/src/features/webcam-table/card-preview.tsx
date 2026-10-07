import { BookOpen, ChevronLeft, ChevronRight, RefreshCw, Trash2, Undo2, X } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { CardImage } from "@/components/card-image"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { ManaCost, ManaSymbol, parseManaCost } from "@/components/mana-symbols"
import { cn } from "@/lib/cn"
import {
  printingCaption,
  preloadImage,
  printingPrices,
  usePrintingDetails,
  type PrintingDetails,
} from "./card-details"
import { CardRulings } from "./card-rulings"
import { isTypingTarget } from "./table-hotkeys"
import { usePreviewPrintings } from "./use-preview-printings"
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

/** Layouts whose second face has its own scan; split and flip halves share the front image. */
const DOUBLE_FACED = new Set(["transform", "modal_dfc", "reversible_card", "double_faced_token"])

/** The same printing's other face: the API addresses the second face with a `-1` suffix. */
function otherFaceId(id: string) {
  return id.endsWith("-1") ? id.slice(0, -2) : `${id}-1`
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
export function CardPreview(props: Props) {
  return <PrintingPreview key={props.card.id} {...props} />
}

function PrintingPreview({ card: initial, ownerName, onWrongCard, onRemove, onClose }: Props) {
  const printings = usePreviewPrintings(initial)
  const card = printings.card
  const shown = usePrintingDetails(card.id)
  const hasBackFace = DOUBLE_FACED.has(shown.data?.layout ?? "")
  const other = usePrintingDetails(hasBackFace ? otherFaceId(card.id) : null)
  // Remembers which printing was flipped, so moving to another printing shows its own face.
  const [flippedId, setFlippedId] = useState<string | null>(null)
  const flipped = hasBackFace && flippedId === card.id
  const details = flipped ? other : shown
  // A recognized card can already be the back face, so label by the face shown.
  const showingBack = (flipped ? otherFaceId(card.id) : card.id).endsWith("-1")
  const otherImage = other.data?.image_uris.normal
  const [rulingsOpen, setRulingsOpen] = useState(false)
  const dialog = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const previous = document.activeElement
    dialog.current?.focus()
    return () => {
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus()
    }
  }, [])

  useEffect(() => {
    function close(event: KeyboardEvent) {
      if (event.key === "Escape" && !event.defaultPrevented && !rulingsOpen) onClose()
    }
    window.addEventListener("keydown", close)
    return () => window.removeEventListener("keydown", close)
  }, [onClose, rulingsOpen])

  useEffect(() => preloadImage(otherImage), [otherImage])

  const data = details.data
  // A board entry names both faces ("A // B"); a double-faced card shows the visible face's.
  const name = hasBackFace ? (data?.name ?? card.name) : card.name
  const caption = data ? printingCaption({ ...data, set: data.set_code }) : printingCaption(card)

  return (
    <>
      <div
        ref={dialog}
        tabIndex={-1}
        className="absolute inset-0 z-20 flex items-center justify-center bg-base-300/90 p-4 outline-none backdrop-blur-[2px]"
        role="dialog"
        aria-modal="true"
        aria-label={`${card.name} details`}
        onClick={onClose}
        onKeyDown={(event) => {
          if (
            rulingsOpen ||
            event.defaultPrevented ||
            event.nativeEvent.isComposing ||
            event.repeat ||
            event.altKey ||
            event.ctrlKey ||
            event.metaKey ||
            event.shiftKey ||
            isTypingTarget(event.target) ||
            !event.currentTarget.contains(document.activeElement)
          )
            return
          if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
            event.preventDefault()
            event.stopPropagation()
            if (event.key === "ArrowLeft") printings.previous()
            else printings.next()
          }
        }}
      >
        <div
          className="pointer-events-none flex max-h-full max-w-full flex-col items-center gap-3"
          onContextMenu={(event) => {
            event.preventDefault()
            setRulingsOpen(true)
          }}
        >
          <div
            className="pointer-events-auto flex max-w-full flex-wrap items-center justify-center gap-2 rounded-xl border border-white/15 bg-base-300 p-2"
            onClick={(event) => event.stopPropagation()}
          >
            {onWrongCard && (
              <button
                type="button"
                className="btn btn-sm h-9 min-h-0 gap-2 rounded-lg border-accent/30 bg-accent/10 px-3 text-xs text-accent hover:bg-accent/20"
                onClick={onWrongCard}
              >
                <Undo2 className="size-3.5" />
                Wrong card?
              </button>
            )}
            {onRemove && (
              <button
                type="button"
                className="btn btn-sm h-9 min-h-0 gap-2 rounded-lg border-white/15 bg-white/5 px-3 text-xs text-white/85 hover:bg-white/15"
                onClick={onRemove}
              >
                <Trash2 className="size-3.5" /> Remove
              </button>
            )}
            <button
              type="button"
              className="btn btn-sm h-9 min-h-0 gap-2 rounded-lg border-white/15 bg-white/5 px-3 text-xs text-white/85 hover:bg-white/15"
              onClick={() => setRulingsOpen(true)}
            >
              <BookOpen className="size-3.5" /> Rulings
            </button>
            <button
              type="button"
              className="btn btn-sm size-9 min-h-0 rounded-lg border-white/15 bg-white/5 p-0 text-white/85 hover:bg-white/15"
              onClick={onClose}
              aria-label="Close card details"
            >
              <X className="size-4" />
            </button>
          </div>

          <div className="flex min-h-0 items-start gap-3">
            <aside
              className="pointer-events-auto hidden max-h-[65dvh] w-72 shrink-0 overflow-y-auto rounded-xl border border-white/10 bg-base-300 p-4 text-sm text-white shadow-2xl md:block"
              aria-label="Rules text"
              onClick={(event) => event.stopPropagation()}
            >
              {data?.mana_cost && <ManaCost cost={data.mana_cost} className="text-base" />}
              <h2 className="mt-1 text-base font-bold">{name}</h2>
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

            <figure
              className="pointer-events-auto relative flex min-h-0 flex-col items-center rounded-2xl border border-white/10 bg-base-300 p-3 shadow-2xl"
              onClick={(event) => event.stopPropagation()}
            >
              <div className={cn("relative w-[min(22rem,60vw,42dvh)]", !data && "animate-pulse")}>
                <CardImage
                  // The listed printing's image is the front, so a loading back face waits.
                  imageUris={data?.image_uris ?? (flipped ? {} : (card.image_uris ?? {}))}
                  name={name}
                  variant="card"
                  className="w-full"
                />
                {hasBackFace && (
                  <button
                    type="button"
                    className="btn btn-sm absolute bottom-3 left-1/2 h-9 min-h-0 -translate-x-1/2 gap-2 rounded-lg border-white/20 bg-base-300/80 px-4 text-xs font-bold text-white backdrop-blur hover:bg-base-300"
                    aria-label={`Show ${showingBack ? "front" : "back"} face of ${card.name}`}
                    onClick={() => setFlippedId(flipped ? null : card.id)}
                  >
                    <RefreshCw className="size-3.5" />
                    {showingBack ? "Show front" : "Show back"}
                  </button>
                )}
              </div>
              <figcaption className="mt-2 text-center text-xs text-white/70">
                <GameChangerBadge gameChanger={data?.game_changer} />
                {data?.game_changer && <br />}
                {caption}
                {ownerName && <span className="text-white/45"> · {ownerName}’s board</span>}
              </figcaption>
              <p className="mt-1 text-center text-xs text-white/85" aria-label="Prices in USD">
                {data ? printingPrices(data.prices) : details.isError ? "—" : "Loading prices…"}
              </p>
              <nav className="mt-2 flex items-center gap-3" aria-label="Card printings">
                <button
                  type="button"
                  aria-label="Previous printing"
                  disabled={printings.total < 2}
                  className="btn btn-sm size-9 min-h-0 rounded-lg border-white/15 bg-white/5 p-0 text-white/85 hover:bg-white/15"
                  onClick={printings.previous}
                >
                  <ChevronLeft className="size-4" />
                </button>
                <span className="min-w-14 text-center text-xs text-white/70" aria-live="polite">
                  {printings.index + 1} / {printings.total}
                </span>
                <button
                  type="button"
                  aria-label="Next printing"
                  disabled={printings.total < 2}
                  className="btn btn-sm size-9 min-h-0 rounded-lg border-white/15 bg-white/5 p-0 text-white/85 hover:bg-white/15"
                  onClick={printings.next}
                >
                  <ChevronRight className="size-4" />
                </button>
              </nav>
              {printings.isPending && (
                <p role="status" className="mt-1 text-xs text-white/50">
                  Loading printings…
                </p>
              )}
              {printings.isError && (
                <p role="status" className="mt-1 text-xs text-white/50">
                  Printings unavailable.{" "}
                  <button type="button" className="underline" onClick={printings.retry}>
                    Retry
                  </button>
                </p>
              )}
            </figure>
          </div>
        </div>
      </div>
      {rulingsOpen && <CardRulings card={card} onClose={() => setRulingsOpen(false)} />}
    </>
  )
}
