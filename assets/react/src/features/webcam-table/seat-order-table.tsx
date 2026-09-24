import { ChevronDown, ChevronUp } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { commanderNames, type DeckSummary } from "@/features/decks/decks"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import type { TimerSample } from "./game-timer"
import { activeTurnOrder } from "./table-events"
import { formatTurnTime, turnDisplay, type TurnState } from "./turns"
import { useTimerElapsed } from "./use-timer-elapsed"
import type { TableParticipant } from "./use-webcam-room"
import type { GameFormat } from "@/features/games/game-format"
import { turnId, unattackableSeats } from "./game-modes"

export interface SeatOrderTableProps {
  mode?: GameFormat
  onMoveSeat?: (peerId: string, delta: -1 | 1) => void
  readOnly?: boolean
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
  mode = "commander",
  onMoveSeat,
  readOnly = false,
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
  // A remounted tab has already missed this event; only animate new events while visible.
  const previousShuffle = useRef(shuffleVersion)
  const elapsed = useTimerElapsed(timer)
  useEffect(() => {
    if (previousShuffle.current === shuffleVersion) return
    previousShuffle.current = shuffleVersion
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
  const protectedSeats =
    mode === "five_star" ? unattackableSeats(participants, localParticipant.player_id) : []
  // Team formats key shared life and turns by seat pair, so their order locks at start.
  const canReorder =
    !readOnly && onMoveSeat != null && (timer?.state.started_at == null || mode === "commander")

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
          {canReorder && (
            <th className="w-7 py-1">
              <span className="sr-only">Reorder</span>
            </th>
          )}
          <th className="w-16 py-1 text-center font-semibold">Turn</th>
          <th className="w-11 py-1 text-right font-semibold">Time</th>
        </tr>
      </thead>
      <tbody>
        {displayed.map((seat) => {
          const active =
            turnId(participants, seat.player_id, mode) === turns.active_player_id &&
            !seat.eliminated
          const display = turnDisplay(turns, seat.player_id, elapsed, participants, mode)
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
                <span className="block text-[0.6rem] text-white/45" title={commander}>
                  {mode === "two_headed_giant"
                    ? `Team ${Math.floor(participants.indexOf(seat) / 2) + 1}`
                    : `${seat.life} life`}
                  {commander && (
                    <>
                      {" · "}
                      <CommanderHover deck={deck}>
                        <span tabIndex={0}>
                          {deck ? <DeckCommanders deck={deck} compact /> : commander}
                        </span>
                      </CommanderHover>
                    </>
                  )}
                </span>
                {protectedSeats.includes(seat.peer_id) && (
                  <span className="block text-[0.6rem] text-warning">Can't attack yet</span>
                )}
                {!readOnly && (
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
                )}
              </td>
              {canReorder && onMoveSeat && (
                <td className="py-2">
                  <div className="flex flex-col items-center gap-0.5">
                    <button
                      type="button"
                      className="btn btn-square btn-outline btn-xs size-5 min-h-0"
                      aria-label={`Move ${seat.player_name} up`}
                      disabled={participants.indexOf(seat) === 0}
                      onClick={() => onMoveSeat(seat.peer_id, -1)}
                    >
                      <ChevronUp className="size-3" />
                    </button>
                    <button
                      type="button"
                      className="btn btn-square btn-outline btn-xs size-5 min-h-0"
                      aria-label={`Move ${seat.player_name} down`}
                      disabled={participants.indexOf(seat) === participants.length - 1}
                      onClick={() => onMoveSeat(seat.peer_id, 1)}
                    >
                      <ChevronDown className="size-3" />
                    </button>
                  </div>
                </td>
              )}
              <td className="py-2 text-center tabular-nums">
                <div className="flex items-center justify-center">
                  {!readOnly && (
                    <button
                      type="button"
                      className="size-5 rounded hover:bg-white/10 disabled:opacity-30"
                      aria-label={`Remove a turn from ${seat.player_name}`}
                      disabled={display.count === 0}
                      onClick={() => onAdjustTurn(seat.player_id, -1)}
                    >
                      −
                    </button>
                  )}
                  <span aria-label={`${seat.player_name} turn count`}>{display.count}</span>
                  {!readOnly && (
                    <button
                      type="button"
                      className="size-5 rounded hover:bg-white/10 disabled:opacity-30"
                      aria-label={`Add a turn to ${seat.player_name}`}
                      disabled={display.count >= 999}
                      onClick={() => onAdjustTurn(seat.player_id, 1)}
                    >
                      +
                    </button>
                  )}
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
