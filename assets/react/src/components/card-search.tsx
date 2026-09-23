import { useQuery } from "@tanstack/react-query"
import { Search, X } from "lucide-react"
import { useEffect, useId, useRef, useState } from "react"
import type { KeyboardEvent } from "react"
import { CardImage } from "./card-image"
import { GameChangerBadge } from "./game-changer-badge"
import { ManaCost } from "./mana-symbols"
import type { CardSummary } from "@/lib/cards"
import { searchCards, type CardSearchMode } from "@/lib/cards"
import { cn } from "@/lib/cn"

interface CardSearchProps {
  value: CardSummary | null
  onChange: (card: CardSummary | null) => void
  /** Which cards to offer: any card, primary-commander-eligible cards, or partner-eligible cards (incl. Backgrounds). */
  mode?: CardSearchMode
  placeholder?: string
  label: string
  required?: boolean
}

export function CardSearch({
  value,
  onChange,
  mode = "all",
  placeholder = "Search cards by name…",
  label,
  required = false,
}: CardSearchProps) {
  const id = useId()
  const listboxId = `${id}-listbox`
  const [query, setQuery] = useState(value?.name ?? "")
  const [debounced, setDebounced] = useState("")
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(-1)
  const clearingSelectionForSearch = useRef(false)

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(query.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [query])

  useEffect(() => {
    if (clearingSelectionForSearch.current && value === null) {
      clearingSelectionForSearch.current = false
      return
    }
    setQuery(value?.name ?? "")
  }, [value])

  const cards = useQuery({
    queryKey: ["cards", { q: debounced, mode }],
    queryFn: () => searchCards(debounced, mode),
    enabled: open && debounced.length > 0,
  })
  const options = cards.data ?? []

  function select(card: CardSummary) {
    onChange(card)
    setQuery(card.name)
    setOpen(false)
    setActive(-1)
  }

  function clear() {
    onChange(null)
    setQuery("")
    setDebounced("")
    setOpen(true)
    setActive(-1)
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault()
      setOpen(true)
      setActive((current) => Math.min(current + 1, options.length - 1))
    } else if (event.key === "ArrowUp") {
      event.preventDefault()
      setActive((current) => Math.max(current - 1, 0))
    } else if (event.key === "Enter" && active >= 0 && options[active]) {
      event.preventDefault()
      select(options[active])
    } else if (event.key === "Escape") {
      setOpen(false)
      setActive(-1)
    }
  }

  return (
    <div className="form-control relative min-w-0 w-full">
      <label htmlFor={id} className="label pb-1 font-medium">
        {label}
      </label>
      <div className="relative">
        <Search
          className="text-base-content/45 pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2"
          aria-hidden="true"
        />
        <input
          id={id}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open && debounced.length > 0}
          aria-controls={listboxId}
          aria-activedescendant={active >= 0 ? `${id}-option-${active}` : undefined}
          autoComplete="off"
          value={query}
          required={required}
          placeholder={placeholder}
          onFocus={() => setOpen(true)}
          onBlur={() => window.setTimeout(() => setOpen(false), 100)}
          onChange={(event) => {
            setQuery(event.target.value)
            if (value) {
              clearingSelectionForSearch.current = true
              onChange(null)
            }
            setOpen(true)
            setActive(-1)
          }}
          onKeyDown={onKeyDown}
          className="input input-bordered min-w-0 w-full pr-10 pl-9"
        />
        {(query || value) && (
          <button
            type="button"
            aria-label="Clear card"
            onMouseDown={(event) => event.preventDefault()}
            onClick={clear}
            className="btn btn-ghost btn-xs absolute top-1/2 right-2 -translate-y-1/2"
          >
            <X className="size-4" aria-hidden="true" />
          </button>
        )}
      </div>

      {value && (
        <div className="border-base-300 bg-base-100 mt-2 flex items-center gap-3 rounded-md border p-2">
          <CardImage
            imageUris={value.image_uris}
            name={value.name}
            className="h-10 w-14 shrink-0"
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-medium">{value.name}</span>
            <GameChangerBadge gameChanger={value.game_changer} />
            <span className="text-base-content/60 block truncate text-xs">{value.type_line}</span>
          </span>
          {value.mana_cost && <ManaCost cost={value.mana_cost} className="shrink-0" />}
        </div>
      )}

      {open && debounced.length > 0 && (
        <ul
          id={listboxId}
          role="listbox"
          aria-label={`${label} results`}
          className="bg-base-100 border-base-300 absolute top-full z-20 mt-1 max-h-80 w-full overflow-y-auto rounded-lg border p-1 shadow-xl"
        >
          {cards.isPending && <li className="text-base-content/60 p-3 text-sm">Searching…</li>}
          {cards.isError && <li className="text-error p-3 text-sm">Card search unavailable.</li>}
          {cards.isSuccess && options.length === 0 && (
            <li className="text-base-content/60 p-3 text-sm">No cards found.</li>
          )}
          {options.map((card, index) => (
            <li
              id={`${id}-option-${index}`}
              key={card.id}
              role="option"
              aria-selected={index === active}
              onMouseDown={(event) => event.preventDefault()}
              onMouseEnter={() => setActive(index)}
              onClick={() => select(card)}
              className={cn(
                "flex cursor-pointer items-center gap-3 rounded-md p-2",
                index === active ? "bg-primary text-primary-content" : "hover:bg-base-200",
              )}
            >
              <CardImage
                imageUris={card.image_uris}
                name={card.name}
                className="h-12 w-16 shrink-0"
              />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{card.name}</span>
                <GameChangerBadge gameChanger={card.game_changer} />
                <span className="block truncate text-xs opacity-70">{card.type_line}</span>
              </span>
              {card.mana_cost && <ManaCost cost={card.mana_cost} className="shrink-0" />}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
