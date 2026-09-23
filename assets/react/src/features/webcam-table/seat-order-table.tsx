import { useEffect, useState } from "react"
import { commanderNames, type DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import type { TimerSample } from "./game-timer"
import { activeTurnOrder } from "./table-events"
import { formatTurnTime, turnDisplay, type TurnState } from "./turns"
import { useTimerElapsed } from "./use-timer-elapsed"
import type { TableParticipant } from "./use-webcam-room"

export interface SeatOrderTableProps {
  participants: TableParticipant[]
  localParticipant: TableParticipant
  decks: DeckSummary[]
  shuffleVersion: number
  turns: TurnState
  timer: TimerSample | null
  onSetEliminated: (peerId: string, eliminated: boolean) => void
  onAdjustTurn: (playerId: number, delta: -1 | 1) => void
}

export function SeatOrderTable({
  participants,
  localParticipant,
  decks,
  shuffleVersion,
  turns,
  timer,
  onSetEliminated,
  onAdjustTurn,
}: SeatOrderTableProps) {
  const [step, setStep] = useState(0)
  const elapsed = useTimerElapsed(timer)
  useEffect(() => {
    if (!shuffleVersion || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return
    let frame = 1
    setStep(frame)
    const interval = window.setInterval(() => {
      frame += 1
      setStep(frame < 7 ? frame : 0)
      if (frame >= 7) window.clearInterval(interval)
    }, 160)
    return () => window.clearInterval(interval)
  }, [shuffleVersion])
  const offset = participants.length ? step % participants.length : 0
  const displayed = [...participants.slice(offset), ...participants.slice(0, offset)]
  const positions = new Map(
    activeTurnOrder(displayed).map((seat, index) => [seat.peer_id, index + 1]),
  )

  return (
    <table
      className="w-full table-fixed text-[0.7rem]"
      aria-label="Turn order"
      aria-busy={step !== 0}
    >
      <thead className="text-base-content/50 text-[0.6rem] tracking-wider uppercase">
        <tr>
          <th className="w-5 py-1 text-left font-semibold">#</th>
          <th className="py-1 text-left font-semibold">Player</th>
          <th className="w-16 py-1 text-center font-semibold">Turn</th>
          <th className="w-11 py-1 text-right font-semibold">Time</th>
        </tr>
      </thead>
      <tbody>
        {displayed.map((seat) => {
          const active = seat.player_id === turns.active_player_id && !seat.eliminated
          const display = turnDisplay(turns, seat.player_id, elapsed)
          const deck = decks.find((candidate) => candidate.id === seat.deck_id)
          const commander = deck ? commanderNames(deck) : seat.deck_name
          return (
            <tr
              key={`${step}-${seat.peer_id}`}
              data-peer={seat.peer_id}
              aria-current={active ? "step" : undefined}
              className={cn(
                "border-t border-white/5",
                active && "bg-amber-400/10",
                seat.eliminated && "text-white/45",
              )}
            >
              <td className="py-2 tabular-nums">{positions.get(seat.peer_id) ?? "—"}</td>
              <td className="py-2 pr-1">
                <span className="flex items-center gap-1 font-semibold">
                  {active && (
                    <span
                      className="size-1.5 shrink-0 rounded-full bg-amber-300"
                      aria-label="Current turn"
                    />
                  )}
                  <span className={cn("truncate", seat.eliminated && "line-through")}>
                    {seat.player_name}
                    {seat.peer_id === localParticipant.peer_id ? " (you)" : ""}
                  </span>
                </span>
                <span className="block truncate text-[0.6rem] text-white/45" title={commander}>
                  {seat.life} life
                  {commander && (
                    <>
                      {" · "}
                      <CommanderHover deck={deck}>
                        <span tabIndex={0}>{commander}</span>
                      </CommanderHover>
                    </>
                  )}
                </span>
                <button
                  type="button"
                  className="mt-1 rounded border border-white/15 px-1 py-0.5 text-[0.6rem] hover:bg-white/10 disabled:opacity-50"
                  aria-pressed={seat.eliminated}
                  aria-label={`Eliminated: ${seat.player_name}`}
                  disabled={seat.departed}
                  title={seat.departed ? "Rejoin to restore this player" : "Toggle elimination"}
                  onClick={() => onSetEliminated(seat.peer_id, !seat.eliminated)}
                >
                  {seat.eliminated ? "Out · Undo" : "Eliminate"}
                </button>
              </td>
              <td className="py-2 text-center tabular-nums">
                <div className="flex items-center justify-center">
                  <button
                    type="button"
                    className="size-5 rounded hover:bg-white/10 disabled:opacity-30"
                    aria-label={`Remove a turn from ${seat.player_name}`}
                    disabled={display.count === 0}
                    onClick={() => onAdjustTurn(seat.player_id, -1)}
                  >
                    −
                  </button>
                  <span aria-label={`${seat.player_name} turn count`}>{display.count}</span>
                  <button
                    type="button"
                    className="size-5 rounded hover:bg-white/10 disabled:opacity-30"
                    aria-label={`Add a turn to ${seat.player_name}`}
                    disabled={display.count >= 999}
                    onClick={() => onAdjustTurn(seat.player_id, 1)}
                  >
                    +
                  </button>
                </div>
              </td>
              <td
                className="py-2 text-right tabular-nums"
                aria-label={`${seat.player_name} turn time`}
              >
                {formatTurnTime(display.milliseconds)}
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
