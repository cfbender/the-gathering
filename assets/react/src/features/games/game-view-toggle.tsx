import { LayoutGrid, Table2 } from "lucide-react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

export type GameView = "cards" | "table"

export function GameViewToggle({
  value,
  onChange,
}: {
  value: GameView
  onChange: (value: GameView) => void
}) {
  return (
    <ToggleGroup
      type="single"
      value={value}
      onValueChange={(next) => {
        if (next === "cards" || next === "table") onChange(next)
      }}
      aria-label="Game view"
      className="join"
    >
      <ToggleGroupItem value="cards" className="btn btn-sm join-item data-[state=on]:btn-primary">
        <LayoutGrid aria-hidden="true" className="size-4" /> Cards
      </ToggleGroupItem>
      <ToggleGroupItem value="table" className="btn btn-sm join-item data-[state=on]:btn-primary">
        <Table2 aria-hidden="true" className="size-4" /> Table
      </ToggleGroupItem>
    </ToggleGroup>
  )
}
