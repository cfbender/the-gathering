import { useNavigate } from "@tanstack/react-router"
import {
  ArrowLeft,
  Crown,
  Dices,
  Flag,
  LogOut,
  Maximize,
  Menu,
  Minimize,
  Play,
  RotateCcw,
  Users,
  X,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { cn } from "@/lib/cn"
import { SeatCell, gridStyle } from "./board-grid"
import { FinishTabletop } from "./finish-tabletop"
import { SeatPanel, type SeatActions } from "./seat-panel"
import { SeatPicker } from "./seat-picker"
import {
  changeCommanderDamage,
  changeLife,
  changePoison,
  chooseFirstSeat,
  findLayout,
  landscape,
  lastStanding,
  passTurn,
  recordBlocker,
  recordBlockerMessage,
  remainingSeats,
  seatName,
  setConceded,
  sit,
  startGame,
  type TabletopGame,
} from "./tabletop"
import { useTabletopGame } from "./tabletop-store"

const ROLL_MS = 1400
const ROLL_STEP_MS = 90

function useLandscape() {
  const query = "(orientation: landscape)"
  const [matches, setMatches] = useState(() => matchMedia(query).matches)
  useEffect(() => {
    const media = matchMedia(query)
    const onChange = () => setMatches(media.matches)
    media.addEventListener("change", onChange)
    return () => media.removeEventListener("change", onChange)
  }, [])
  return matches
}

/** Keeps the screen on while the board is up; the lock drops whenever the page is hidden, so
 * it is taken again on return. */
function useWakeLock() {
  useEffect(() => {
    if (!("wakeLock" in navigator)) return
    let lock: WakeLockSentinel | null = null
    let cancelled = false
    const acquire = () => {
      if (document.visibilityState !== "visible") return
      navigator.wakeLock
        .request("screen")
        .then((sentinel) => {
          if (cancelled) void sentinel.release()
          else lock = sentinel
        })
        .catch(() => {
          // Battery saver or an unfocused page can refuse; the board works without it.
        })
    }
    acquire()
    document.addEventListener("visibilitychange", acquire)
    return () => {
      cancelled = true
      document.removeEventListener("visibilitychange", acquire)
      void lock?.release()
    }
  }, [])
}

function useElapsed(startedAt: string | null) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!startedAt) return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [startedAt])
  if (!startedAt) return null
  const seconds = Math.max(0, Math.floor((now - Date.parse(startedAt)) / 1000))
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const pad = (value: number) => String(value).padStart(2, "0")
  return hours > 0
    ? `${hours}:${pad(minutes)}:${pad(seconds % 60)}`
    : `${minutes}:${pad(seconds % 60)}`
}

/** The whole-screen board: seats around the edges, each facing its player, and a hub in the
 * middle for starting, turns, and ending the game. */
export function TabletopBoard({ game }: { game: TabletopGame }) {
  const navigate = useNavigate()
  const [, setGame] = useTabletopGame()
  const isLandscape = useLandscape()
  const portrait = findLayout(game.seats.length, game.layoutId)
  const layout = isLandscape ? landscape(portrait) : portrait
  const [menuOpen, setMenuOpen] = useState(false)
  const [finishing, setFinishing] = useState(false)
  const [confirmRestart, setConfirmRestart] = useState(false)
  const [askFirst, setAskFirst] = useState(game.phase === "playing" && game.firstSeat === null)
  const [rolling, setRolling] = useState<number | null>(null)
  // The last-standing prompt can be waved off; it comes back if someone else wins instead.
  const [dismissedWinner, setDismissedWinner] = useState<number | null>(null)
  // Seats whose chooser or counters are open; the hub steps aside so it never covers them.
  const [openSheets, setOpenSheets] = useState<number[]>([])
  const sheetChange = useCallback(
    (index: number, open: boolean) =>
      setOpenSheets((current) =>
        open
          ? current.includes(index)
            ? current
            : [...current, index]
          : current.filter((at) => at !== index),
      ),
    [],
  )
  const sheetHandlers = useMemo(
    () => game.seats.map((_, index) => (open: boolean) => sheetChange(index, open)),
    [game.seats.length, sheetChange],
  )
  const elapsed = useElapsed(game.phase === "playing" ? game.startedAt : null)
  const standing = lastStanding(game)
  const blocker = recordBlocker(game)
  useWakeLock()

  const start = () => {
    setGame(startGame(game))
    setAskFirst(true)
    setDismissedWinner(null)
    setFinishing(false)
    setMenuOpen(false)
  }

  const roll = () => {
    const candidates = remainingSeats(game)
    const pick = candidates[Math.floor(Math.random() * candidates.length)]!
    const startedAt = Date.now()
    let step = 0
    setRolling(candidates[0]!)
    const timer = window.setInterval(() => {
      step += 1
      if (Date.now() - startedAt >= ROLL_MS) {
        window.clearInterval(timer)
        setRolling(null)
        setAskFirst(false)
        setGame((current) => chooseFirstSeat(current, pick))
        return
      }
      setRolling(candidates[step % candidates.length]!)
    }, ROLL_STEP_MS)
  }

  const actionsFor = (index: number): SeatActions => ({
    onLife: (delta) => setGame((current) => changeLife(current, index, delta)),
    onPoison: (delta) => setGame((current) => changePoison(current, index, delta)),
    onCommanderDamage: (key, delta) =>
      setGame((current) => changeCommanderDamage(current, index, key, delta)),
    onConcede: (conceded) => setGame((current) => setConceded(current, index, conceded)),
    onPassTurn: () => setGame((current) => passTurn(current)),
  })

  const takenPlayerIds = game.seats.flatMap((seat) =>
    seat.occupant?.kind === "player" ? [seat.occupant.id] : [],
  )

  return (
    <div
      className="fixed inset-0 grid touch-none gap-1.5 overscroll-none bg-black p-1.5 select-none"
      style={{
        ...gridStyle(layout),
        paddingTop: "max(0.375rem, env(safe-area-inset-top))",
        paddingBottom: "max(0.375rem, env(safe-area-inset-bottom))",
        paddingLeft: "max(0.375rem, env(safe-area-inset-left))",
        paddingRight: "max(0.375rem, env(safe-area-inset-right))",
      }}
      onContextMenu={(event) => event.preventDefault()}
    >
      {layout.seats.map((area, index) => {
        const seat = game.seats[index]
        if (!seat) return null
        return (
          <SeatCell
            key={`${index}-${game.startedAt ?? "seating"}`}
            area={area}
            label={seatName(seat, index)}
          >
            {game.phase === "seating" ? (
              <SeatPicker
                seat={seat}
                index={index}
                format={game.format}
                takenPlayerIds={takenPlayerIds.filter(
                  (id) => seat.occupant?.kind !== "player" || seat.occupant.id !== id,
                )}
                onSit={(occupant, deck) =>
                  setGame((current) => sit(current, index, occupant, deck))
                }
                onSheetChange={sheetHandlers[index]!}
              />
            ) : (
              <SeatPanel
                game={game}
                index={index}
                highlighted={rolling === index}
                actions={actionsFor(index)}
                onSheetChange={sheetHandlers[index]!}
              />
            )}
          </SeatCell>
        )
      })}

      <div
        className={cn(
          "pointer-events-none absolute inset-0 z-20 grid place-items-center transition-opacity",
          openSheets.length > 0 && "invisible opacity-0",
        )}
      >
        <div className="pointer-events-auto flex flex-col items-center gap-2">
          {game.phase === "seating" ? (
            <SeatingHub
              notice={blocker ? recordBlockerMessage(blocker) : null}
              onStart={start}
              onBack={() => void navigate({ to: "/tabletop" })}
            />
          ) : askFirst ? (
            <HubCard>
              <Dices className="text-accent size-8" />
              <p className="font-black">Who goes first?</p>
              <div className="flex gap-2">
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  disabled={rolling !== null}
                  onClick={() => setAskFirst(false)}
                >
                  Skip
                </button>
                <button
                  type="button"
                  className="btn btn-accent btn-sm"
                  disabled={rolling !== null}
                  onClick={roll}
                >
                  {rolling !== null ? "Rolling…" : "Roll"}
                </button>
              </div>
            </HubCard>
          ) : standing !== null && standing !== dismissedWinner && !finishing ? (
            <HubCard>
              <Crown className="text-accent size-8" />
              <p className="max-w-48 truncate text-lg font-black">
                {seatName(game.seats[standing]!, standing)} wins
              </p>
              <div className="flex gap-2">
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => setDismissedWinner(standing)}
                >
                  Keep playing
                </button>
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  onClick={() => setFinishing(true)}
                >
                  Finish
                </button>
              </div>
            </HubCard>
          ) : (
            <button
              type="button"
              className="border-primary bg-base-100 text-base-content flex h-12 items-center gap-2 rounded-full border-2 px-4 font-black tabular-nums shadow-xl"
              onClick={() => setMenuOpen(true)}
              aria-label="Game menu"
            >
              <Menu className="size-5" />
              {elapsed}
              {game.turn > 0 && (
                <span className="badge badge-accent badge-sm font-black">T{game.turn}</span>
              )}
            </button>
          )}
        </div>
      </div>

      {menuOpen && (
        <HubMenu
          game={game}
          onClose={() => setMenuOpen(false)}
          onPickFirst={() => {
            setMenuOpen(false)
            setAskFirst(true)
          }}
          onRestart={() => setConfirmRestart(true)}
          onSeats={() => {
            setMenuOpen(false)
            setGame((current) => ({ ...current, phase: "seating" }))
          }}
          onFinish={() => {
            setMenuOpen(false)
            setFinishing(true)
          }}
          onLeave={() => void navigate({ to: "/tabletop" })}
        />
      )}

      <ConfirmDialog
        open={confirmRestart}
        onOpenChange={setConfirmRestart}
        title="Restart the game?"
        confirmLabel="Restart"
        destructive
        onConfirm={start}
      >
        Everyone goes back to {game.startingLife} life and the clock starts over.
      </ConfirmDialog>

      {finishing && (
        <FinishTabletop
          game={game}
          onOpenChange={setFinishing}
          onRematch={start}
          onClose={() => setGame(null)}
        />
      )}
    </div>
  )
}

function HubCard({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-box border-base-300 bg-base-100 text-base-content flex flex-col items-center gap-2 border px-5 py-4 text-center shadow-2xl">
      {children}
    </div>
  )
}

function SeatingHub({
  notice,
  onStart,
  onBack,
}: {
  notice: string | null
  onStart: () => void
  onBack: () => void
}) {
  return (
    <div className="flex max-w-64 flex-col items-center gap-2">
      <div className="flex items-center gap-2">
        <button
          type="button"
          className="btn btn-circle bg-base-100 border-base-300 shadow-xl"
          aria-label="Back to setup"
          onClick={onBack}
        >
          <ArrowLeft className="size-5" />
        </button>
        <button
          type="button"
          className="btn btn-primary btn-lg rounded-full px-6 shadow-xl"
          onClick={onStart}
        >
          <Play className="size-5" fill="currentColor" /> Start game
        </button>
      </div>
      {notice && (
        <p className="rounded-field bg-base-100/90 text-base-content/75 px-3 py-1.5 text-center text-xs shadow">
          {notice}
        </p>
      )}
    </div>
  )
}

function HubMenu({
  game,
  onClose,
  onPickFirst,
  onRestart,
  onSeats,
  onFinish,
  onLeave,
}: {
  game: TabletopGame
  onClose: () => void
  onPickFirst: () => void
  onRestart: () => void
  onSeats: () => void
  onFinish: () => void
  onLeave: () => void
}) {
  const [fullscreen, setFullscreen] = useState(() => document.fullscreenElement !== null)
  useEffect(() => {
    const onChange = () => setFullscreen(document.fullscreenElement !== null)
    document.addEventListener("fullscreenchange", onChange)
    return () => document.removeEventListener("fullscreenchange", onChange)
  }, [])
  const item = "btn btn-ghost h-12 justify-start gap-3 text-base"

  return (
    <div
      className="absolute inset-0 z-30 grid place-items-center bg-black/60 p-4 backdrop-blur-sm"
      onClick={onClose}
    >
      <nav
        aria-label="Game menu"
        className="rounded-box border-base-300 bg-base-100 text-base-content grid w-full max-w-xs gap-1 border p-2 shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between px-2 py-1">
          <span className="text-sm font-black">
            {game.activeSeat !== null
              ? `Turn ${game.turn} · ${seatName(game.seats[game.activeSeat]!, game.activeSeat)}`
              : "Game menu"}
          </span>
          <button
            type="button"
            className="btn btn-ghost btn-sm btn-square"
            aria-label="Close menu"
            onClick={onClose}
          >
            <X className="size-4" />
          </button>
        </div>
        <button type="button" className={item} onClick={onPickFirst}>
          <Dices className="size-5" />
          {game.firstSeat === null ? "Roll for first player" : "Roll first player again"}
        </button>
        <button type="button" className={item} onClick={onRestart}>
          <RotateCcw className="size-5" /> Restart game
        </button>
        <button type="button" className={item} onClick={onSeats}>
          <Users className="size-5" /> Change seats
        </button>
        {document.fullscreenEnabled && (
          <button
            type="button"
            className={item}
            onClick={() => {
              if (fullscreen) void document.exitFullscreen()
              else void document.documentElement.requestFullscreen().catch(() => undefined)
            }}
          >
            {fullscreen ? <Minimize className="size-5" /> : <Maximize className="size-5" />}
            {fullscreen ? "Exit full screen" : "Full screen"}
          </button>
        )}
        <button type="button" className={cn(item, "text-primary")} onClick={onFinish}>
          <Flag className="size-5" /> End game
        </button>
        <button type="button" className={cn(item, "text-base-content/70")} onClick={onLeave}>
          <LogOut className="size-5" /> Leave (game stays saved)
        </button>
      </nav>
    </div>
  )
}
