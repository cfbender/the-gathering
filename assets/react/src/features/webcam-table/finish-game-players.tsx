import type { Dispatch, SetStateAction } from "react"
import { MvpCardField } from "@/components/mvp-card-field"
import { assignKill, KillPicker } from "@/features/games/kill-picker"
import type { PlayerResult } from "./game-result"

export function FinishGamePlayers({
  players,
  setPlayers,
  disabled,
}: {
  players: PlayerResult[]
  setPlayers: Dispatch<SetStateAction<PlayerResult[]>>
  disabled: boolean
}) {
  function update(playerId: number, patch: Partial<PlayerResult>) {
    setPlayers((current) =>
      current.map((seat) => (seat.playerId === playerId ? { ...seat, ...patch } : seat)),
    )
  }

  return (
    <fieldset className="min-w-0 space-y-3 md:col-span-2" disabled={disabled}>
      <legend className="mb-2 text-sm font-bold">Kills & MVP cards</legend>
      <p className="text-base-content/60 text-sm">
        Optional for each player. Select named victims or enter only a total if you forgot who.
      </p>
      {players.map((seat) => (
        <fieldset key={seat.playerId} className="border-base-300 min-w-0 rounded-xl border p-3">
          <legend className="px-1 text-sm font-bold">{seat.playerName}</legend>
          <div className="grid min-w-0 gap-3 sm:grid-cols-2">
            <KillPicker
              seat={seat}
              seats={players}
              onVictimChange={(victimId, checked) =>
                setPlayers((current) => assignKill(current, seat.draftId, victimId, checked))
              }
              onTotalChange={(kills) => update(seat.playerId, { kills })}
            />
            <MvpCardField
              value={seat.mvpCard}
              onChange={(mvpCard) => update(seat.playerId, { mvpCard })}
            />
          </div>
        </fieldset>
      ))}
    </fieldset>
  )
}
