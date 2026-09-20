import { useQuery } from "@tanstack/react-query"
import { LoaderCircle } from "lucide-react"
import { useState } from "react"
import { CommanderField } from "@/components/commander-field"
import { DecklistUrlField } from "@/components/decklist-url-field"
import { ColorIdentity } from "@/components/mana-symbols"
import { combinedColorIdentity, type SelectedCard } from "@/lib/cards"
import { detailsFromDecklist, useResolveDecklist, type Decklist } from "@/lib/decklists"
import { remoteDecksQueryOptions, remoteDeckSourceLabels } from "@/lib/remote-decks"

export interface DeckFormValue {
  commander: SelectedCard | null
  partner: SelectedCard | null
  colorIdentity: string
  decklistUrl: string
}

interface DeckFormFieldsProps {
  value: DeckFormValue
  onChange: (patch: Partial<DeckFormValue>) => void
  onResolvedName?: (name: string) => void
}

export function DeckFormFields({ value, onChange, onResolvedName }: DeckFormFieldsProps) {
  const [applyingDecklist, setApplyingDecklist] = useState(false)

  function changeCard(field: "commander" | "partner", card: SelectedCard | null) {
    const cards = field === "commander" ? [card, value.partner] : [value.commander, card]
    onChange({ [field]: card, colorIdentity: combinedColorIdentity(cards) })
  }

  async function applyDecklist(decklist: Decklist) {
    setApplyingDecklist(true)
    try {
      const details = await detailsFromDecklist(decklist)
      onResolvedName?.(details.name)
      onChange(details)
    } finally {
      setApplyingDecklist(false)
    }
  }

  return (
    <>
      <CommanderField
        value={value.commander}
        onChange={(card) => changeCard("commander", card)}
        required
      />
      <CommanderField
        label="Partner (optional)"
        value={value.partner}
        onChange={(card) => changeCard("partner", card)}
      />
      <label className="form-control min-w-0">
        <span className="label-text mb-1 text-sm font-medium">Color identity</span>
        <span className="relative">
          <input
            className="input input-bordered min-w-0 w-full pr-24 font-mono uppercase"
            value={value.colorIdentity}
            placeholder="WUBRG"
            pattern="(?!.*(.).*\\1)[WUBRG]*"
            onChange={(event) => onChange({ colorIdentity: event.target.value.toUpperCase() })}
          />
          <ColorIdentity
            colors={value.colorIdentity}
            className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-lg"
          />
        </span>
      </label>
      <div className="min-w-0 sm:col-span-2">
        <RemoteDeckPicker onPick={(decklist) => void applyDecklist(decklist)} />
        <DecklistUrlField
          value={value.decklistUrl}
          onChange={(decklistUrl) => onChange({ decklistUrl })}
          onResolved={(decklist) => void applyDecklist(decklist)}
        />
        {applyingDecklist && (
          <p className="text-base-content/60 mt-1 text-sm" role="status">
            Looking up commanders…
          </p>
        )}
      </div>
    </>
  )
}

function RemoteDeckPicker({ onPick }: { onPick: (decklist: Decklist) => void }) {
  const remoteDecks = useQuery(remoteDecksQueryOptions)
  const resolve = useResolveDecklist()
  const [selectedUrl, setSelectedUrl] = useState("")
  const decks = remoteDecks.data?.decks ?? []

  if (decks.length === 0) return null

  return (
    <div className="bg-primary/5 border-primary/20 mb-3 rounded-box border p-3">
      <label className="form-control flex flex-col gap-1.5">
        <span className="text-sm font-medium">Quick pick from my hosted decks</span>
        <select
          className="select select-bordered select-sm w-full"
          value={selectedUrl}
          disabled={resolve.isPending}
          onChange={(event) => {
            const url = event.target.value
            if (!url) return
            setSelectedUrl(url)
            resolve.mutate(url, { onSuccess: onPick })
          }}
        >
          <option value="">Choose a public deck…</option>
          {decks.map((deck) => (
            <option key={`${deck.source}:${deck.url}`} value={deck.url}>
              {deck.name} · {remoteDeckSourceLabels[deck.source]}
            </option>
          ))}
        </select>
      </label>
      {resolve.isPending && (
        <p className="text-base-content/60 mt-2 flex items-center gap-1.5 text-xs" role="status">
          <LoaderCircle className="size-3 animate-spin" /> Loading deck details…
        </p>
      )}
      {resolve.isError && (
        <p className="text-error mt-2 text-xs" role="alert">
          That deck could not be loaded. Open its link to check that it is public.
        </p>
      )}
    </div>
  )
}
