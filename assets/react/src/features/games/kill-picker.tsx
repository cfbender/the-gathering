import { ChevronDown } from "lucide-react"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import type { DraftSeat } from "./use-game-draft"

interface KillPickerProps {
  seat: DraftSeat
  seats: DraftSeat[]
  onVictimChange: (victimId: string, checked: boolean) => void
  onTotalChange: (value: string) => void
}

export function KillPicker({ seat, seats, onVictimChange, onTotalChange }: KillPickerProps) {
  const victims = seats.filter((victim) => victim.eliminatedByDraftId === seat.draftId)
  const unknown = seat.kills === "" ? null : Math.max(0, Number(seat.kills) - victims.length)
  const summary = [
    ...victims.map((victim) => victim.playerName || "Unnamed player"),
    ...(unknown ? [`${unknown} unknown`] : []),
  ].join(", ")

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <span className="text-xs font-medium">Kills (optional)</span>
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="input input-bordered input-sm flex w-full items-center justify-between gap-2 text-left"
            aria-label={`Kills for ${seat.playerName || "unnamed player"}`}
          >
            <span className="truncate">
              {summary || (seat.kills === "0" ? "No kills" : "Select players…")}
            </span>
            <ChevronDown className="size-4 shrink-0" />
          </button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          aria-label={`Victims of ${seat.playerName || "this player"}`}
          className="w-72 max-w-[calc(100vw-2rem)] p-3"
        >
          <p className="mb-2 text-sm font-semibold">
            Who did {seat.playerName || "this player"} kill?
          </p>
          <div className="flex flex-col gap-1">
            {seats
              .filter((victim) => victim.draftId !== seat.draftId)
              .map((victim) => {
                const killer = seats.find(
                  (candidate) => candidate.draftId === victim.eliminatedByDraftId,
                )
                const taken = Boolean(killer && killer.draftId !== seat.draftId)
                return (
                  <label
                    key={victim.draftId}
                    className="flex cursor-pointer items-center gap-3 rounded-lg p-2 hover:bg-base-200 has-disabled:cursor-default has-disabled:opacity-50"
                  >
                    <input
                      type="checkbox"
                      className="checkbox checkbox-sm"
                      aria-label={victim.playerName || "Unnamed player"}
                      aria-description={taken ? `Killed by ${killer?.playerName}` : undefined}
                      checked={victim.eliminatedByDraftId === seat.draftId}
                      disabled={taken || !victim.playerName.trim()}
                      onChange={(event) => onVictimChange(victim.draftId, event.target.checked)}
                    />
                    <span className="min-w-0 text-sm">
                      {victim.playerName || "Unnamed player"}
                      {taken && (
                        <span className="block text-xs">Killed by {killer?.playerName}</span>
                      )}
                    </span>
                  </label>
                )
              })}
          </div>
          <p className="mt-2 text-xs text-base-content/60">
            Forgot who? Enter only the total. Each player can be selected by one killer.
          </p>
        </PopoverContent>
      </Popover>
      <label className="flex items-center gap-2 text-xs text-base-content/70">
        <span>Total kills</span>
        <input
          type="number"
          min={victims.length}
          max="9"
          step="1"
          aria-label="Total kills"
          placeholder="?"
          className="input input-bordered input-sm w-20"
          value={seat.kills}
          onChange={(event) => onTotalChange(event.target.value)}
        />
        <span>
          {victims.length} named{unknown ? ` · ${unknown} unknown` : ""}
        </span>
      </label>
    </div>
  )
}
