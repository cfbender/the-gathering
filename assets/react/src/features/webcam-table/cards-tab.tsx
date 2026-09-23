import { Eraser, Search, Sparkles, WalletCards, X } from "lucide-react"
import { useEffect, useState } from "react"
import { ManaCost } from "@/components/mana-symbols"
import { cn } from "@/lib/cn"
import { CardThumb } from "./board-cards"
import { printingCaption, usePrintingDetails } from "./card-details"
import { PanelSection } from "./panel-section"
import { galleryPrintingCaption, type GalleryArt } from "./recognition/pipeline"
import type { BoardCard, IdentifiedCard, TableParticipant } from "./use-webcam-room"

export interface CardsTabProps {
  participants: TableParticipant[]
  localParticipant: TableParticipant
  identifiedCards: BoardCard[]
  /** The worker has the gallery, so names can be searched even without a camera click. */
  gallerySearchable: boolean
  onSearch: (query: string) => Promise<GalleryArt[]>
  onPreviewCard: (entry: BoardCard) => void
  onPreviewArt: (art: GalleryArt) => void
  onRemoveCard: (id: string) => void
  /** Clears everything identified on the local seat's own board, at every seat. */
  onClearOwnCards: () => void
}

function CardMeta({ card }: { card: IdentifiedCard }) {
  const details = usePrintingDetails(card.id)
  const data = details.data
  return (
    <>
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="truncate font-semibold">{card.name}</span>
        {data?.mana_cost && <ManaCost cost={data.mana_cost} className="shrink-0 text-[0.65rem]" />}
      </span>
      <span className="text-base-content/55 block truncate text-[0.65rem]">
        {data
          ? printingCaption({ ...data, set: data.set_code })
          : printingCaption({ set: card.set, collector_number: card.collector_number })}
      </span>
    </>
  )
}

function CardRow({
  card,
  onPreview,
  onRemove,
}: {
  card: IdentifiedCard
  onPreview: () => void
  onRemove?: () => void
}) {
  return (
    <li className="flex items-center gap-2">
      <CardThumb card={card} onClick={onPreview} className="w-8 shrink-0" />
      <button
        type="button"
        className="min-w-0 flex-1 text-left text-xs hover:underline"
        onClick={onPreview}
      >
        <CardMeta card={card} />
      </button>
      {onRemove && (
        <button
          type="button"
          className="text-base-content/50 hover:text-error grid size-6 shrink-0 place-items-center rounded-full hover:bg-white/10"
          onClick={onRemove}
          aria-label={`Remove ${card.name}`}
        >
          <X className="size-3.5" />
        </button>
      )}
    </li>
  )
}

function LatestCard({
  entry,
  ownerName,
  onPreview,
  onClear,
}: {
  entry: BoardCard
  ownerName: string
  onPreview: () => void
  onClear: () => void
}) {
  const details = usePrintingDetails(entry.card.id)
  const data = details.data
  return (
    <div className="flex gap-3">
      <CardThumb card={entry.card} onClick={onPreview} className="w-20 shrink-0" />
      <div className="min-w-0 flex-1 text-xs">
        {data?.mana_cost && <ManaCost cost={data.mana_cost} className="text-sm" />}
        <button type="button" className="block text-left hover:underline" onClick={onPreview}>
          <span className="block font-bold">{entry.card.name}</span>
        </button>
        <p className="text-base-content/70 truncate">{data?.type_line ?? "Loading…"}</p>
        <p className="text-base-content/55 truncate text-[0.65rem]">
          {data
            ? printingCaption({ ...data, set: data.set_code })
            : printingCaption({
                set: entry.card.set,
                collector_number: entry.card.collector_number,
              })}
          {" · "}
          {ownerName}’s board
        </p>
        <button
          type="button"
          className="btn btn-ghost btn-xs mt-1.5 -ml-2 text-[0.65rem]"
          onClick={onClear}
        >
          Clear
        </button>
      </div>
    </div>
  )
}

function GallerySearch({
  onSearch,
  onPick,
}: {
  onSearch: CardsTabProps["onSearch"]
  onPick: (art: GalleryArt) => void
}) {
  const [query, setQuery] = useState("")
  const [matches, setMatches] = useState<GalleryArt[]>([])
  const [status, setStatus] = useState("")

  useEffect(() => {
    if (query.trim() === "") {
      setMatches([])
      setStatus("")
      return
    }
    let stale = false
    setStatus("Loading gallery…")
    onSearch(query)
      .then((arts) => {
        if (!stale) {
          setMatches(arts)
          setStatus(arts.length ? "" : "No matching printings.")
        }
      })
      .catch(() => {
        if (!stale) setStatus("Gallery unavailable. Check Settings > Card scan.")
      })
    return () => {
      stale = true
    }
  }, [onSearch, query])

  return (
    <div className="px-3 pb-3">
      <label className="flex h-8 items-center gap-2 rounded-field border border-white/10 bg-white/5 px-2 text-xs focus-within:border-white/40">
        <Search className="text-base-content/50 size-3.5" />
        <input
          className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-base-content/40"
          placeholder="Search name, set:fin, #274, lang:en"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          aria-label="Search the card gallery"
        />
        {query && (
          <button
            type="button"
            className="text-base-content/50 hover:text-base-content"
            onClick={() => setQuery("")}
            aria-label="Clear search"
          >
            <X className="size-3.5" />
          </button>
        )}
      </label>
      {status && (
        <p role="status" className="mt-2 text-xs text-base-content/60">
          {status}
        </p>
      )}
      {matches.length > 0 && (
        <ul
          className="mt-1 max-h-48 overflow-y-auto rounded-field border border-white/10 text-xs"
          aria-label="Search results"
        >
          {matches.map((art) => (
            <li key={art.id}>
              <button
                type="button"
                className="w-full px-2 py-1.5 text-left hover:bg-white/10"
                onClick={() => onPick(art)}
              >
                <span className="block truncate font-semibold">{art.name}</span>
                <span className="text-base-content/55 block text-[0.65rem]">
                  {galleryPrintingCaption(art)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Side-panel tab for everything the table has identified: the newest card in full, a gallery
 * search that previews any printing, and the detected cards grouped by whose board they are
 * on, for the whole table or just your own board. */
export function CardsTab({
  participants,
  localParticipant,
  identifiedCards,
  gallerySearchable,
  onSearch,
  onPreviewCard,
  onPreviewArt,
  onRemoveCard,
  onClearOwnCards,
}: CardsTabProps) {
  const [clearedLatestId, setClearedLatestId] = useState<string | null>(null)
  const [scope, setScope] = useState<"shared" | "mine">("shared")

  const latest = identifiedCards.at(-1)
  const nameFor = (peerId: string) =>
    participants.find((participant) => participant.peer_id === peerId)?.player_name ??
    "A departed player"
  const shown =
    scope === "mine"
      ? identifiedCards.filter((entry) => entry.ownerPeerId === localParticipant.peer_id)
      : identifiedCards
  const groups = new Map<string, BoardCard[]>()
  for (const entry of [...shown].reverse()) {
    const list = groups.get(entry.ownerPeerId) ?? []
    list.push(entry)
    groups.set(entry.ownerPeerId, list)
  }

  return (
    <>
      <PanelSection title="Latest card" icon={Sparkles}>
        {latest && latest.id !== clearedLatestId ? (
          <LatestCard
            entry={latest}
            ownerName={nameFor(latest.ownerPeerId)}
            onPreview={() => onPreviewCard(latest)}
            onClear={() => setClearedLatestId(latest.id)}
          />
        ) : (
          <p className="text-base-content/55 text-xs">
            Click a card on a board to identify it. The newest one shows up here.
          </p>
        )}
      </PanelSection>

      {gallerySearchable && <GallerySearch onSearch={onSearch} onPick={onPreviewArt} />}

      <PanelSection
        title="Detected cards"
        icon={WalletCards}
        meta={
          <span
            role="group"
            aria-label="Which boards"
            className="flex rounded-full border border-white/15 p-0.5 text-[0.6rem]"
            onClick={(event) => event.stopPropagation()}
          >
            {(["shared", "mine"] as const).map((option) => (
              <button
                key={option}
                type="button"
                className={cn(
                  "rounded-full px-2 py-0.5 font-semibold",
                  scope === option ? "bg-primary text-primary-content" : "text-base-content/60",
                )}
                onClick={() => setScope(option)}
                aria-pressed={scope === option}
              >
                {option === "shared" ? "Shared" : "Mine"}
              </button>
            ))}
          </span>
        }
      >
        {groups.size === 0 ? (
          <p className="text-base-content/55 text-xs">
            {scope === "mine"
              ? "Nothing has been identified on your board yet."
              : "Nothing has been identified at this table yet."}
          </p>
        ) : (
          <div className="grid gap-3">
            {[...groups.entries()].map(([peerId, entries]) => (
              <section key={peerId} aria-label={`Cards on ${nameFor(peerId)}'s board`}>
                <div className="mb-1 flex items-center justify-between gap-2">
                  <h3 className="text-base-content/50 text-[0.6rem] font-bold tracking-wider uppercase">
                    {nameFor(peerId)} ({entries.length})
                  </h3>
                  {peerId === localParticipant.peer_id && (
                    <button
                      type="button"
                      className="btn btn-ghost btn-xs -my-1 gap-1 text-[0.6rem] hover:text-error"
                      onClick={onClearOwnCards}
                    >
                      <Eraser className="size-3" /> Clear cards
                    </button>
                  )}
                </div>
                <ul className="grid gap-1.5">
                  {entries.map((entry) => (
                    <CardRow
                      key={entry.id}
                      card={entry.card}
                      onPreview={() => onPreviewCard(entry)}
                      onRemove={() => onRemoveCard(entry.id)}
                    />
                  ))}
                </ul>
              </section>
            ))}
          </div>
        )}
      </PanelSection>
    </>
  )
}
