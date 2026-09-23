import { Check, ChevronDown } from "lucide-react"
import { useState, type ReactNode } from "react"
import { ColorIdentity } from "@/components/mana-symbols"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { commanderNames, type DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import { CommanderActions } from "./new-commander-dialog"

interface Props {
  playerId: number
  playerName: string
  decks: DeckSummary[]
  selectedDeckId?: number
  onChoose: (deckId: number) => void
  /** Rendered as the trigger; defaults to a compact "Select commander" button. */
  children?: ReactNode
  align?: "start" | "center" | "end"
}

/** Lists one player's recorded decks so any seat at the table can set that player's commander.
 * The channel rejects decks that do not belong to the seated player. */
export function CommanderPicker({
  playerId,
  playerName,
  decks,
  selectedDeckId,
  onChoose,
  children,
  align = "end",
}: Props) {
  const [open, setOpen] = useState(false)
  const selected = decks.find((deck) => deck.id === selectedDeckId)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        {children ?? (
          <button
            type="button"
            className={cn(
              "btn btn-xs h-auto min-h-6 min-w-0 max-w-full shrink gap-1 border-0 px-2 py-0.5 font-semibold",
              selected ? "btn-ghost text-white/85" : "btn-primary",
            )}
          >
            <CommanderHover deck={open ? undefined : selected}>
              <span
                className="min-w-0 max-w-40 text-left leading-tight"
                title={selected && commanderNames(selected)}
              >
                {selected ? (
                  <>
                    <span className="block truncate">{selected.commander_name}</span>
                    {selected.partner_name && (
                      <span className="block truncate">/ {selected.partner_name}</span>
                    )}
                  </>
                ) : (
                  "Select commander"
                )}
              </span>
            </CommanderHover>
            {selected && <ColorIdentity colors={selected.color_identity} />}
            <ChevronDown className="size-3 shrink-0 opacity-70" />
          </button>
        )}
      </PopoverTrigger>
      <PopoverContent align={align} className="w-72 p-2">
        <div className="text-base-content/55 px-2 pt-1 pb-2 text-[0.65rem] font-bold tracking-wider uppercase">
          {playerName}’s commanders
        </div>
        {decks.length === 0 ? (
          <p className="text-base-content/65 px-2 pb-2 text-sm">
            No decks are recorded for {playerName} yet.
          </p>
        ) : (
          <ul className="max-h-72 overflow-y-auto">
            {decks.map((deck) => (
              <li key={deck.id}>
                <CommanderHover deck={deck}>
                  <button
                    type="button"
                    className="hover:bg-base-200 flex w-full items-center gap-2 rounded-field px-2 py-1.5 text-left text-sm"
                    onClick={() => {
                      onChoose(deck.id)
                      setOpen(false)
                    }}
                  >
                    <Check
                      className={cn("size-3.5 shrink-0", deck.id !== selectedDeckId && "invisible")}
                    />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-1 font-semibold">
                        <span className="min-w-0">{commanderNames(deck)}</span>
                        <ColorIdentity colors={deck.color_identity} />
                      </span>
                      <span className="text-base-content/55 block truncate text-xs">
                        {deck.name}
                      </span>
                    </span>
                  </button>
                </CommanderHover>
              </li>
            ))}
          </ul>
        )}
        <CommanderActions
          playerId={playerId}
          deck={selected}
          onChoose={(id) => {
            onChoose(id)
            setOpen(false)
          }}
        />
      </PopoverContent>
    </Popover>
  )
}
