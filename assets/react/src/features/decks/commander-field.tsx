import { CardSearch } from "@/components/card-search"
import { PrintingPicker } from "./printing-picker"
import type { CardSearchMode, CardSummary, SelectedCard } from "@/lib/cards"
import { cardForDisplay, selectCatalogCard } from "@/lib/cards"

interface CommanderFieldProps {
  value: SelectedCard | null
  onChange: (value: SelectedCard | null) => void
  required?: boolean
  label?: string
  allowPrintings?: boolean
  /** `"partner"` allows any commander-eligible card or Background; `"all"` allows any card. */
  mode?: CardSearchMode
}

export function CommanderField({
  value,
  onChange,
  required,
  label = "Commander",
  allowPrintings = false,
  mode = "commander",
}: CommanderFieldProps) {
  const change = (card: CardSummary | null) => onChange(card ? selectCatalogCard(card) : null)

  return (
    <div className="min-w-0">
      <CardSearch
        label={label}
        value={cardForDisplay(value)}
        onChange={change}
        mode={mode}
        placeholder={`Search for a ${label.toLowerCase()}…`}
        required={required}
      />
      {allowPrintings && value && (
        <PrintingPicker
          key={value.id}
          card={value}
          label={mode === "commander" ? "Commander" : "Partner"}
          onChange={(printing_id) => onChange({ ...value, printing_id })}
        />
      )}
    </div>
  )
}
