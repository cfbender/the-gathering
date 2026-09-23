import { Link } from "@tanstack/react-router"
import {
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  DoorOpen,
  Gamepad2,
  Layers,
  ScanSearch,
  ScrollText,
  Settings,
  Shuffle,
  Users,
  WalletCards,
  Wifi,
} from "lucide-react"
import type { ComponentType, ReactNode } from "react"
import { commanderNames, type DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import { CardsTab, type CardsTabProps } from "./cards-tab"
import { CommanderPicker } from "./commander-picker"
import { CommanderActions } from "./new-commander-dialog"
import type { TimerSample } from "./game-timer"
import { PanelSection } from "./panel-section"
import { RevealControl } from "./reveal-control"
import type { RecognizerState } from "./recognition/use-recognizer"
import { SeatOrderTable } from "./seat-order-table"
import { TableRolls, type RollRequest } from "./table-rolls"
import { TimerBadge, TimerToggle } from "./table-timer"
import { nextActiveSeat, type TurnState } from "./turns"
import type { TableEvent, TableParticipant } from "./use-webcam-room"

export type PanelTab = "table" | "decks" | "cards" | "log" | "settings"

interface Props extends CardsTabProps {
  left?: boolean
  settings: ReactNode
  onHelp: () => void
  open: boolean
  tab: PanelTab
  onOpenChange: (open: boolean) => void
  onTabChange: (tab: PanelTab) => void
  participants: TableParticipant[]
  localParticipant: TableParticipant
  maxPlayers: number
  playerDecks: DeckSummary[]
  /** Every deck at the table, so remote seats' commanders can be named from their deck_id. */
  decks: DeckSummary[]
  events: TableEvent[]
  status: string
  error: string | null
  connectedPeers: number
  connectionStates: Record<string, RTCPeerConnectionState>
  iceServers: RTCIceServer[]
  recognizer: RecognizerState
  onInvite: () => void
  inviteCopied: boolean
  onChooseDeck: (deckId: number) => void
  onRandomizeSeats: () => void
  shuffleVersion: number
  turns: TurnState
  timer: TimerSample | null
  autoRandomize: boolean
  onAutoRandomize: (enabled: boolean) => void
  onPassTurn: () => void
  onAdjustTurn: (playerId: number, delta: -1 | 1) => void
  onRoll: (request: RollRequest) => void
  onSetEliminated: (peerId: string, eliminated: boolean) => void
  onEndGame: () => void
  onChangeTimer: (action: "pause" | "resume") => void
  /** Private reveal: who the local camera is currently shown to, if anyone. */
  reveal: {
    target: string | null
    busy: boolean
    onChange: (target: string | null) => Promise<void>
  }
}

const TABS: { id: PanelTab; label: string; icon: ComponentType<{ className?: string }> }[] = [
  { id: "table", label: "Table", icon: Gamepad2 },
  { id: "decks", label: "Decks", icon: Layers },
  { id: "cards", label: "Cards", icon: WalletCards },
  { id: "log", label: "Log", icon: ScrollText },
  { id: "settings", label: "Settings", icon: Settings },
]

export function describeIceServers(servers: RTCIceServer[]): string {
  const urls = servers.flatMap((server) =>
    Array.isArray(server.urls) ? server.urls : [server.urls],
  )
  const stun = urls.filter((url) => url.startsWith("stun:")).length
  const turn = urls.filter((url) => url.startsWith("turn:") || url.startsWith("turns:")).length
  if (stun === 0 && turn === 0) return "no STUN or TURN — same network only"
  const parts = []
  if (stun > 0) parts.push(`${stun} STUN`)
  if (turn > 0) parts.push(`${turn} TURN`)
  return parts.join(", ") + (turn === 0 ? " (no relay)" : "")
}

export function describeRecognizer(state: RecognizerState): string {
  switch (state.status) {
    case "checking":
      return "Checking for a recognition bundle…"
    case "unavailable":
      return "No recognition bundle is installed on this server; clicks offer the seat's decks instead. Publish one with `python -m cardid.publish`."
    case "loading":
      return `Loading bundle ${state.version}…`
    case "ready":
      return `Bundle ${state.version}: ${state.arts.toLocaleString()} artworks, loaded in ${(state.loadMs / 1000).toFixed(1)} s.`
    case "failed":
      return `Recognizer failed to start: ${state.message}`
  }
}

function RecognizerBadge({ state }: { state: RecognizerState }) {
  const tone =
    state.status === "ready"
      ? "badge-success"
      : state.status === "failed"
        ? "badge-error"
        : state.status === "unavailable"
          ? "badge-ghost"
          : "badge-warning"
  const label =
    state.status === "ready"
      ? "ready"
      : state.status === "failed"
        ? "failed"
        : state.status === "unavailable"
          ? "off"
          : "loading"
  return <span className={cn("badge badge-xs", tone)}>{label}</span>
}

function commanderName(participant: TableParticipant, decks: DeckSummary[]) {
  const deck = decks.find((deck) => deck.id === participant.deck_id)
  return deck ? commanderNames(deck) : participant.deck_name
}

function TableTab(props: Props) {
  const {
    participants,
    localParticipant: local,
    maxPlayers,
    playerDecks,
    decks,
    status,
    error,
    connectedPeers,
    connectionStates,
    iceServers,
    recognizer,
    onInvite,
    inviteCopied,
    onChooseDeck,
    onRandomizeSeats,
    onEndGame,
  } = props
  const started = props.timer?.state.started_at != null

  return (
    <>
      <PanelSection
        title="Setup"
        icon={Gamepad2}
        meta={
          <span className="text-base-content/60 flex items-center gap-2 text-[0.65rem] font-semibold">
            <TimerBadge sample={props.timer} />
            <span className="flex items-center gap-1">
              <Users className="size-3" /> {participants.length}/{maxPlayers}
            </span>
          </span>
        }
      >
        <div className="grid gap-1.5">
          <button
            type="button"
            className="btn btn-outline btn-sm w-full text-xs"
            onClick={onInvite}
          >
            {inviteCopied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
            {inviteCopied ? "Invite link copied" : "Invite players"}
          </button>
          <CommanderPicker
            playerId={local.player_id}
            playerName={local.player_name}
            decks={playerDecks}
            selectedDeckId={local.deck_id}
            onChoose={onChooseDeck}
            align="start"
          >
            <button
              type="button"
              className="btn btn-outline btn-sm h-auto min-h-8 w-full py-1.5 text-xs"
            >
              <Layers className="size-3.5" />
              <CommanderHover deck={playerDecks.find((deck) => deck.id === local.deck_id)}>
                <span className="min-w-0 flex-1 whitespace-normal">
                  {commanderName(local, playerDecks) ?? "Select your commander"}
                </span>
              </CommanderHover>
            </button>
          </CommanderPicker>
        </div>

        <h3 className="text-base-content/50 mt-4 mb-1 text-[0.6rem] font-bold tracking-wider uppercase">
          Turn order
        </h3>
        <SeatOrderTable
          participants={participants}
          localParticipant={local}
          decks={decks}
          shuffleVersion={props.shuffleVersion}
          onSetEliminated={props.onSetEliminated}
          turns={props.turns}
          timer={props.timer}
          onAdjustTurn={props.onAdjustTurn}
        />
        <p className="text-base-content/50 mt-1 text-[0.65rem]">
          Out players skip turns but keep their recorded seat. Any player can eliminate or restore a
          seat.
        </p>

        <div className="mt-3 grid gap-1.5">
          {!started && (
            <>
              <label className="text-base-content/60 mb-1 flex items-center justify-between gap-2 text-[0.65rem]">
                Auto-randomize order on start
                <input
                  type="checkbox"
                  className="toggle toggle-xs toggle-primary"
                  checked={props.autoRandomize}
                  onChange={(event) => props.onAutoRandomize(event.target.checked)}
                />
              </label>
              <button
                type="button"
                className="btn btn-primary btn-sm w-full text-xs"
                onClick={onRandomizeSeats}
                disabled={participants.filter((participant) => !participant.departed).length < 2}
              >
                <Shuffle className="size-3.5" />{" "}
                {props.autoRandomize ? "Randomize and start" : "Start match"}
              </button>
            </>
          )}
          {started && (
            <button
              type="button"
              className="btn btn-outline btn-sm w-full text-xs"
              onClick={props.onPassTurn}
              disabled={props.turns.active_player_id === null}
              title={`Next: ${nextActiveSeat(participants, props.turns.active_player_id)?.player_name ?? "No eligible players"}`}
            >
              Pass turn <kbd className="kbd kbd-xs">Space</kbd>
            </button>
          )}
          <TimerToggle sample={props.timer} onChange={props.onChangeTimer} />
          <RevealControl
            participants={participants}
            peerId={local.peer_id}
            target={props.reveal.target}
            busy={props.reveal.busy}
            onChange={props.reveal.onChange}
          />
          <button
            type="button"
            className="btn btn-error btn-sm w-full text-xs"
            onClick={onEndGame}
            disabled={participants.length < 2}
          >
            <DoorOpen className="size-3.5" /> End game
          </button>
          <Link to="/games" className="btn btn-ghost btn-sm w-full text-xs">
            Leave table
          </Link>
        </div>
      </PanelSection>

      <PanelSection
        title="Identify cards"
        icon={ScanSearch}
        defaultOpen={false}
        meta={<RecognizerBadge state={recognizer} />}
      >
        <p className="text-base-content/70 text-xs leading-relaxed">
          Click a card on any board. The camera owner returns a native 640 px crop, the recognizer
          runs in your browser, and a clear match opens the card with its rules text and lands in
          that board's tray (the tab at the bottom of the video) at every seat. Say "Wrong card?" on
          the preview, or Shift+click, to pick from its top five instead: press{" "}
          <kbd className="kbd kbd-xs">1</kbd>–<kbd className="kbd kbd-xs">5</kbd> or{" "}
          <kbd className="kbd kbd-xs">/</kbd> to search by name, set code or collector number. The
          Cards tab lists everything identified at the table.
        </p>
        <p className="text-base-content/50 mt-2 text-xs">{describeRecognizer(recognizer)}</p>
      </PanelSection>

      <TableRolls onRoll={props.onRoll} />

      <PanelSection title="Connection" icon={Wifi} defaultOpen={false}>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-base-content/50">Status</dt>
          <dd className={cn(error && "text-error")}>{error ?? status}</dd>
          <dt className="text-base-content/50">Peers</dt>
          <dd>
            {connectedPeers} of {Math.max(0, participants.length - 1)} streaming
          </dd>
          <dt className="text-base-content/50">Video</dt>
          <dd>1080p mesh, up to {maxPlayers} players</dd>
          <dt className="text-base-content/50">ICE</dt>
          <dd>{describeIceServers(iceServers)}</dd>
        </dl>
        {Object.values(connectionStates).some((state) => state === "failed") && (
          <p className="text-warning mt-2 text-xs leading-relaxed">
            A peer couldn't be reached directly. Players on different networks usually need a TURN
            relay: set <code>CLOUDFLARE_TURN_KEY_ID</code> and{" "}
            <code>CLOUDFLARE_TURN_API_TOKEN</code> or the <code>WEBRTC_TURN_*</code> variables on
            the server (see docs/webcam-table.md).
          </p>
        )}
      </PanelSection>
    </>
  )
}

function DecksTab({ playerDecks, localParticipant: local, onChooseDeck }: Props) {
  return (
    <PanelSection title="Your commanders" icon={Layers}>
      {playerDecks.length === 0 ? (
        <p className="text-base-content/65 text-xs">No decks are recorded for you yet.</p>
      ) : (
        <ul className="grid gap-1">
          {playerDecks.map((deck) => {
            const selected = deck.id === local.deck_id
            return (
              <li key={deck.id}>
                <CommanderHover deck={deck}>
                  <button
                    type="button"
                    className={cn(
                      "flex w-full items-center gap-2 rounded-field border px-2 py-1.5 text-left text-xs",
                      selected
                        ? "border-primary bg-primary/15"
                        : "border-white/10 hover:border-white/30",
                    )}
                    onClick={() => onChooseDeck(deck.id)}
                    aria-pressed={selected}
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block font-semibold">{commanderNames(deck)}</span>
                      <span className="text-base-content/55 block truncate">{deck.name}</span>
                    </span>
                    {selected && <Check className="text-primary size-3.5" />}
                  </button>
                </CommanderHover>
              </li>
            )
          })}
        </ul>
      )}
      <CommanderActions
        playerId={local.player_id}
        deck={playerDecks.find((deck) => deck.id === local.deck_id)}
        onChoose={onChooseDeck}
      />
    </PanelSection>
  )
}

function LogTab({ events }: Props) {
  return (
    <PanelSection title="Table log" icon={ScrollText}>
      {events.length === 0 ? (
        <p className="text-base-content/55 text-xs">Nothing has happened yet.</p>
      ) : (
        <ol className="grid gap-1 text-xs">
          {events.map((event) => (
            <li key={event.id} className="flex gap-2">
              <time
                className="text-base-content/45 shrink-0 tabular-nums"
                dateTime={event.at.toISOString()}
              >
                {event.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </time>
              <span>
                {event.text}
                {event.count && <span className="ml-1 text-white/40">×{event.count}</span>}
              </span>
            </li>
          ))}
        </ol>
      )}
    </PanelSection>
  )
}

/** Right-hand control column: a narrow icon strip that switches tabs and collapses the panel,
 * plus the stacked, collapsible sections for the active tab. */
export function SidePanel(props: Props) {
  const { open, tab, onOpenChange, onTabChange } = props
  const Content =
    tab === "table" ? TableTab : tab === "decks" ? DecksTab : tab === "cards" ? CardsTab : LogTab

  return (
    <div
      className={cn(
        "bg-base-100 text-base-content flex max-h-[45dvh] flex-col border-t border-white/10 lg:max-h-none lg:flex-row lg:border-t-0 lg:border-l",
        props.left && "lg:order-1 lg:flex-row-reverse",
      )}
    >
      <nav
        className="flex shrink-0 items-center gap-1 px-1.5 py-1 lg:w-14 lg:flex-col lg:items-stretch lg:px-1 lg:py-1.5"
        aria-label="Table panels"
      >
        <button
          type="button"
          className="text-base-content/60 hover:bg-white/10 hover:text-base-content grid h-7 place-items-center rounded"
          onClick={() => onOpenChange(!open)}
          aria-label={open ? "Collapse panel" : "Expand panel"}
          aria-expanded={open}
        >
          {open ? (
            <ChevronRight className="size-4 -rotate-90 lg:rotate-0" />
          ) : (
            <ChevronLeft className="size-4 -rotate-90 lg:rotate-0" />
          )}
        </button>
        {TABS.map(({ id, label, icon: Icon }) => {
          const active = open && tab === id
          return (
            <button
              key={id}
              type="button"
              className={cn(
                "flex flex-col items-center gap-0.5 rounded px-2 py-1.5 text-[0.55rem] font-semibold",
                active
                  ? "bg-primary/20 text-primary"
                  : "text-base-content/60 hover:bg-white/10 hover:text-base-content",
              )}
              onClick={() => {
                onTabChange(id)
                onOpenChange(true)
              }}
              aria-pressed={active}
            >
              <Icon className="size-4" />
              {label}
            </button>
          )
        })}
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={props.onHelp}
          aria-label="Keyboard shortcuts"
          title="Keyboard shortcuts (?)"
        >
          ?
        </button>
      </nav>
      {open && (
        <div className="bg-base-200 min-h-0 min-w-0 flex-1 overflow-y-auto border-t border-white/10 lg:w-[var(--table-panel-width)] lg:flex-none lg:border-t-0 lg:border-l">
          {props.error && (
            <div
              role="alert"
              className="bg-error/20 text-error border-b border-white/10 px-3 py-2 text-xs"
            >
              {props.error}
            </div>
          )}
          {tab === "settings" ? props.settings : <Content {...props} />}
        </div>
      )}
    </div>
  )
}
