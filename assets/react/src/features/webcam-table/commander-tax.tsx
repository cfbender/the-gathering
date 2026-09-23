import { ChevronDown, Layers, Minus } from "lucide-react"
import { useState } from "react"
import { ColorIdentity } from "@/components/mana-symbols"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import { CommanderPicker } from "./commander-picker"
import type { Counter } from "./seat-counters"
import type { TableParticipant } from "./use-webcam-room"

const COLOR_TEXT: Record<string, string> = {
  W: "text-amber-100",
  U: "text-sky-300",
  B: "text-violet-300",
  R: "text-red-300",
  G: "text-emerald-300",
  C: "text-slate-300",
  "": "text-white/85",
}

function TaxThumbnail({
  name,
  art,
  casts,
  local,
  onAdjust,
}: {
  name: string
  art: string | null
  casts: number
  local: boolean
  onAdjust: (delta: number) => void
}) {
  const [failed, setFailed] = useState(false)
  return (
    <div className="flex shrink-0 items-center gap-0.5">
      <button
        type="button"
        className="relative grid size-7 shrink-0 place-items-center rounded-md border border-white/20 bg-base-300 text-white/60 enabled:hover:border-primary focus-visible:outline-2 focus-visible:outline-primary"
        aria-label={`${name} commander tax: ${casts * 2}`}
        title={
          local
            ? `${name}: click to add 2 tax; right-click to subtract 2`
            : `${name}: ${casts * 2} commander tax`
        }
        disabled={!local}
        onClick={() => {
          if (casts < 999) onAdjust(1)
        }}
        onContextMenu={(event) => {
          if (!local) return
          event.preventDefault()
          if (casts > 0) onAdjust(-1)
        }}
      >
        {art && !failed ? (
          <img
            src={art}
            alt=""
            className="size-full rounded-md object-cover"
            onError={() => setFailed(true)}
          />
        ) : (
          <Layers className="size-4" />
        )}
        <span className="absolute -right-1 -bottom-1 min-w-3.5 rounded border border-white/25 bg-base-100 px-0.5 text-center text-[0.6rem] leading-3 font-bold text-white tabular-nums">
          {casts * 2}
        </span>
      </button>
      {local && (
        <button
          type="button"
          className="grid size-4 shrink-0 place-items-center rounded text-white/65 hover:bg-white/10 disabled:text-white/20"
          aria-label={`Decrease ${name} commander tax`}
          title="Subtract 2 tax"
          disabled={casts === 0}
          onClick={() => onAdjust(-1)}
        >
          <Minus className="size-3" />
        </button>
      )}
    </div>
  )
}

/** Art badges share the existing cast counters; the name still opens the deck picker. */
export function CommanderTax({
  participant,
  decks,
  local,
  onChooseDeck,
  onAdjust,
}: {
  participant: TableParticipant
  decks: DeckSummary[]
  local: boolean
  onChooseDeck: (deckId: number) => void
  onAdjust: (counter: Counter, delta: number) => void
}) {
  const deck = decks.find((candidate) => candidate.id === participant.deck_id)
  const commanders = deck
    ? [
        { name: deck.commander_name, art: deck.commander_art_crop_url },
        ...(deck.partner_name ? [{ name: deck.partner_name, art: deck.partner_art_crop_url }] : []),
      ]
    : []
  const label = commanders.map(({ name }) => name).join(" / ")

  return (
    <div className="flex min-w-0 items-center gap-1.5">
      {commanders.map(({ name, art }) => (
        <TaxThumbnail
          key={`${name}:${art}`}
          name={name}
          art={art}
          casts={participant.commander_casts[name] ?? 0}
          local={local}
          onAdjust={(delta) => onAdjust({ kind: "casts", commander: name }, delta)}
        />
      ))}
      {local ? (
        <CommanderPicker
          playerId={participant.player_id}
          playerName={participant.player_name}
          decks={decks}
          selectedDeckId={participant.deck_id}
          onChoose={onChooseDeck}
        >
          <button
            type="button"
            className={cn(
              "flex min-w-0 items-center gap-1 rounded px-1 py-1 text-xs font-semibold hover:bg-white/10",
              deck ? (COLOR_TEXT[deck.color_identity] ?? "text-amber-300") : "text-primary",
            )}
            title={label || "Select commander"}
            aria-label={`Choose ${participant.player_name}'s commander`}
          >
            <CommanderHover deck={deck}>
              <span className="truncate">{label || "Select commander"}</span>
            </CommanderHover>
            {deck && <ColorIdentity colors={deck.color_identity} />}
            <ChevronDown className="size-3 shrink-0 opacity-70" />
          </button>
        </CommanderPicker>
      ) : (
        // Only a seat's owner picks its commander; other seats just see it.
        <span
          className={cn(
            "flex min-w-0 items-center gap-1 px-1 py-1 text-xs font-semibold",
            deck ? (COLOR_TEXT[deck.color_identity] ?? "text-amber-300") : "text-white/45",
          )}
          title={label || undefined}
        >
          <CommanderHover deck={deck}>
            <span className="truncate">{label || "No commander yet"}</span>
          </CommanderHover>
          {deck && <ColorIdentity colors={deck.color_identity} />}
        </span>
      )}
    </div>
  )
}
