import { useQuery } from "@tanstack/react-query"
import { Check, ExternalLink, LayoutGrid, List, ListChecks, Search } from "lucide-react"
import { useMemo, useState } from "react"
import { CardHover } from "@/components/card-hover"
import { GameChangerBadge } from "@/components/game-changer-badge"
import { ManaCost } from "@/components/mana-symbols"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import {
  decklistCardsQuery,
  decklistSize,
  groupDecklist,
  hasDecklistCards,
  type DecklistCard,
} from "@/features/decks/decklist-cards"
import type { DeckSummary } from "@/features/decks/decks"
import { ApiError } from "@/lib/api"
import { cn } from "@/lib/cn"
import { inDeck } from "./deck-hint"

const SOURCE_LABELS = { moxfield: "Moxfield", archidekt: "Archidekt", manavault: "ManaVault" }

type DecklistView = "list" | "images"

/** "View decklist" under the chosen commander: opens the linked list in a dialog. Hidden when
 * the deck links nowhere the server can read. */
export function DecklistButton({
  deck,
  boardNames,
}: {
  deck: DeckSummary | undefined
  /** Names already identified on your board, checked off in the list. */
  boardNames: ReadonlySet<string>
}) {
  const [open, setOpen] = useState(false)
  // Shares the table's cache: the list is usually loaded (and its images warm) already.
  const query = useQuery({
    ...decklistCardsQuery(deck?.id ?? 0),
    enabled: !!deck && hasDecklistCards(deck),
  })
  if (!deck || !hasDecklistCards(deck)) return null
  return (
    <>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="mt-2 w-full justify-start gap-2 text-xs"
        onClick={() => setOpen(true)}
      >
        <ListChecks className="size-3.5" />
        View decklist
        <span className="text-base-content/55 ml-auto tabular-nums">
          {query.data ? `${decklistSize(query.data.cards)} cards` : query.isPending ? "…" : ""}
        </span>
      </Button>
      <DecklistDialog deck={deck} open={open} onOpenChange={setOpen} boardNames={boardNames} />
    </>
  )
}

export function DecklistDialog({
  deck,
  open,
  onOpenChange,
  boardNames,
}: {
  deck: DeckSummary
  open: boolean
  onOpenChange: (open: boolean) => void
  boardNames: ReadonlySet<string>
}) {
  const query = useQuery({ ...decklistCardsQuery(deck.id), enabled: open })
  const [filter, setFilter] = useState("")
  const [view, setView] = useState<DecklistView>("list")
  const list = query.data
  const sections = useMemo(() => {
    const needle = filter.trim().toLowerCase()
    const cards = (list?.cards ?? []).filter(
      (card) =>
        needle === "" ||
        card.name.toLowerCase().includes(needle) ||
        (card.type_line ?? "").toLowerCase().includes(needle),
    )
    return groupDecklist(cards)
  }, [filter, list])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-4xl" aria-label={`${deck.name} decklist`}>
        <DialogHeader>
          <div className="min-w-0">
            <DialogTitle className="truncate">{deck.name}</DialogTitle>
            {list && (
              <p className="text-base-content/65 mt-0.5 flex flex-wrap items-center gap-x-2 text-sm">
                <span>{decklistSize(list.cards)} cards</span>
                <span aria-hidden="true">·</span>
                <a
                  href={list.url}
                  target="_blank"
                  rel="noreferrer"
                  className="link inline-flex items-center gap-1"
                >
                  {SOURCE_LABELS[list.source]} <ExternalLink className="size-3" />
                </a>
              </p>
            )}
          </div>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>

        <div className="flex flex-wrap items-center gap-2 border-b border-base-300 px-5 py-3">
          <label className="input input-sm flex min-w-48 flex-1 items-center gap-2">
            <Search className="text-base-content/50 size-3.5" />
            <input
              className="min-w-0 flex-1"
              placeholder="Filter by name or type"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              aria-label="Filter the decklist"
            />
          </label>
          <ToggleGroup
            type="single"
            value={view}
            onValueChange={(next) => {
              if (next === "list" || next === "images") setView(next)
            }}
            aria-label="Decklist view"
            className="join"
          >
            <ToggleGroupItem
              value="list"
              className="btn btn-sm join-item data-[state=on]:btn-primary"
            >
              <List aria-hidden="true" className="size-4" /> List
            </ToggleGroupItem>
            <ToggleGroupItem
              value="images"
              className="btn btn-sm join-item data-[state=on]:btn-primary"
            >
              <LayoutGrid aria-hidden="true" className="size-4" /> Images
            </ToggleGroupItem>
          </ToggleGroup>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {query.isPending && (
            <p role="status" className="text-base-content/65 flex items-center gap-2 text-sm">
              <span className="loading loading-spinner loading-sm" /> Loading decklist…
            </p>
          )}
          {query.isError && (
            <div role="alert" className="alert alert-warning text-sm">
              {query.error instanceof ApiError && query.error.status === 404
                ? "This list is missing or private on the deck site."
                : "The deck site did not answer. Try again in a moment."}
              {!(query.error instanceof ApiError && query.error.status === 404) && (
                <Button type="button" size="sm" onClick={() => void query.refetch()}>
                  Retry
                </Button>
              )}
            </div>
          )}
          {list && sections.length === 0 && (
            <p className="text-base-content/65 text-sm">No cards match “{filter}”.</p>
          )}
          <div className={cn(view === "list" && "gap-x-6 sm:columns-2 lg:columns-3")}>
            {sections.map(({ group, cards }) => (
              <section key={group} className="mb-4 break-inside-avoid">
                <h3 className="text-base-content/60 mb-1 text-xs font-bold tracking-wider uppercase">
                  {group} <span className="tabular-nums">({decklistSize(cards)})</span>
                </h3>
                {view === "list" ? (
                  <ul className="grid gap-0.5">
                    {cards.map((card) => (
                      <DecklistRow
                        key={`${card.zone}:${card.name}`}
                        card={card}
                        onBoard={inDeck(card.name, boardNames)}
                      />
                    ))}
                  </ul>
                ) : (
                  <ul className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-2">
                    {cards.map((card) => (
                      <DecklistImage
                        key={`${card.zone}:${card.name}`}
                        card={card}
                        onBoard={inDeck(card.name, boardNames)}
                      />
                    ))}
                  </ul>
                )}
              </section>
            ))}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function DecklistRow({ card, onBoard }: { card: DecklistCard; onBoard: boolean }) {
  return (
    <li>
      <CardHover
        id={null}
        name={card.name}
        imageUrl={card.image_uris.normal ?? card.image_uris.small}
        gameChanger={card.game_changer}
      >
        <span
          className={cn(
            "flex w-full items-center gap-2 rounded-field px-1.5 py-1 text-sm hover:bg-white/5",
            onBoard && "text-base-content/50",
          )}
          tabIndex={0}
        >
          <span className="text-base-content/55 w-5 shrink-0 text-right tabular-nums">
            {card.quantity}
          </span>
          <span className="min-w-0 flex-1 truncate">{card.name}</span>
          {card.game_changer && <GameChangerBadge gameChanger compact />}
          {onBoard && <Check className="text-secondary size-3.5 shrink-0" aria-label="On board" />}
          {card.mana_cost && <ManaCost cost={card.mana_cost} className="shrink-0 text-[0.7rem]" />}
        </span>
      </CardHover>
    </li>
  )
}

function DecklistImage({ card, onBoard }: { card: DecklistCard; onBoard: boolean }) {
  const src = card.image_uris.normal ?? card.image_uris.small
  return (
    <li className="relative">
      {src ? (
        <img
          src={src}
          alt={card.name}
          loading="lazy"
          decoding="async"
          className={cn(
            "aspect-[5/7] w-full rounded-[4.5%] bg-black object-cover",
            onBoard && "opacity-60",
          )}
        />
      ) : (
        <span className="grid aspect-[5/7] w-full place-items-center rounded-[4.5%] bg-white/10 p-2 text-center text-xs">
          {card.name}
        </span>
      )}
      {card.quantity > 1 && (
        <span className="badge badge-sm badge-neutral absolute top-1 left-1 tabular-nums">
          ×{card.quantity}
        </span>
      )}
      {onBoard && (
        <span className="badge badge-sm badge-secondary absolute top-1 right-1 gap-1">
          <Check className="size-3" /> On board
        </span>
      )}
    </li>
  )
}
