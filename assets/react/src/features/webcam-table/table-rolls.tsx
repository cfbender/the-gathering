import { Dice5 } from "lucide-react"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { PanelSection } from "./panel-section"

export type RollRequest = { kind: "dice"; sides: number } | { kind: "coin" }
export type TableRoll = RollRequest & {
  id: string
  actor: string
  player_name: string
  result: number | string
  at: number
}

export function describeRoll(roll: TableRoll): string {
  return roll.kind === "dice"
    ? `${roll.player_name} rolled a d${roll.sides}: ${roll.result}`
    : `${roll.player_name} flipped a coin: ${roll.result}`
}

export function TableRolls({ onRoll }: { onRoll: (request: RollRequest) => void }) {
  const [sides, setSides] = useState("100")
  return (
    <PanelSection title="Dice & coins" icon={Dice5}>
      <div className="grid grid-cols-3 gap-1.5">
        {[6, 20].map((size) => (
          <Button
            key={size}
            type="button"
            variant="outline"
            size="sm"
            onClick={() => onRoll({ kind: "dice", sides: size })}
          >
            d{size}
          </Button>
        ))}
        <Button type="button" variant="outline" size="sm" onClick={() => onRoll({ kind: "coin" })}>
          Flip coin
        </Button>
      </div>
      <form
        className="mt-2 flex items-end gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          onRoll({ kind: "dice", sides: Number(sides) })
        }}
      >
        <label className="min-w-0 flex-1 text-xs text-white/60">
          Custom sides (2–1000)
          <input
            className="input input-sm mt-1 w-full"
            type="number"
            min="2"
            max="1000"
            step="1"
            required
            value={sides}
            onChange={(event) => setSides(event.target.value)}
          />
        </label>
        <Button type="submit" variant="outline" size="sm">
          Roll
        </Button>
      </form>
    </PanelSection>
  )
}
