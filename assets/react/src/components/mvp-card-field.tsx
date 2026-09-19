import { CardSearch } from "@/components/card-search"
import type { CardSummary, SelectedCard } from "@/lib/cards"
import { selectCatalogCard } from "@/lib/cards"

interface MvpCardFieldProps {
  value: SelectedCard | null
  onChange: (value: SelectedCard | null) => void
}

export function MvpCardField({ value, onChange }: MvpCardFieldProps) {
  const change = (card: CardSummary | null) => onChange(card ? selectCatalogCard(card) : null)
  return <CardSearch label="MVP card (optional)" value={value} onChange={change} />
}
