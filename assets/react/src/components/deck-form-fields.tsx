import { useState } from "react"
import { CommanderField } from "@/components/commander-field"
import { DecklistUrlField } from "@/components/decklist-url-field"
import { combinedColorIdentity, type SelectedCard } from "@/lib/cards"
import { detailsFromDecklist, type Decklist } from "@/lib/decklists"

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
        <input
          className="input input-bordered min-w-0 w-full font-mono uppercase"
          value={value.colorIdentity}
          placeholder="WUBRG"
          pattern="(?!.*(.).*\\1)[WUBRG]*"
          onChange={(event) => onChange({ colorIdentity: event.target.value.toUpperCase() })}
        />
      </label>
      <div className="min-w-0 sm:col-span-2">
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
