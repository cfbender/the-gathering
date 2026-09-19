import { CardSearch } from "@/components/card-search"
import type { CardSummary, SelectedCard } from "@/lib/cards"
import { selectCatalogCard } from "@/lib/cards"

interface CommanderFieldProps {
  value: SelectedCard | null
  onChange: (value: SelectedCard | null) => void
  required?: boolean
  label?: string
}

export function CommanderField({
  value,
  onChange,
  required,
  label = "Commander",
}: CommanderFieldProps) {
  const change = (card: CardSummary | null) => onChange(card ? selectCatalogCard(card) : null)
  return (
    <CardSearch
      label={label}
      value={value}
      onChange={change}
      commanderOnly
      placeholder={`Search for a ${label.toLowerCase()}…`}
      required={required}
    />
  )
}
