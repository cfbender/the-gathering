import { Link } from "@tanstack/react-router"
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  DoorOpen,
  Gamepad2,
  Layers,
  ScanSearch,
  ScrollText,
  Shuffle,
  Users,
  Wifi,
} from "lucide-react"
import { useState, type ComponentType, type ReactNode } from "react"
import type { DeckSummary } from "@/features/decks/decks"
import { cn } from "@/lib/cn"
import { CommanderPicker } from "./commander-picker"
import type { RecognizerState } from "./recognition/use-recognizer"
import type { TableEvent, TableParticipant } from "./use-webcam-room"

export type PanelTab = "table" | "decks" | "log"

interface Props {
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
  recognizer: RecognizerState
  onInvite: () => void
  inviteCopied: boolean
  onChooseDeck: (deckId: number) => void
  onRandomizeSeats: () => void
  onEndGame: () => void
}

const TABS: { id: PanelTab; label: string; icon: ComponentType<{ className?: string }> }[] = [
  { id: "table", label: "Table", icon: Gamepad2 },
  { id: "decks", label: "Decks", icon: Layers },
  { id: "log", label: "Log", icon: ScrollText },
]

function describeRecognizer(state: RecognizerState): string {
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

function PanelSection({
  title,
  icon: Icon,
  meta,
  defaultOpen = true,
  children,
}: {
  title: string
  icon: ComponentType<{ className?: string }>
  meta?: ReactNode
  defaultOpen?: boolean
  children: ReactNode
}) {
  const [expanded, setExpanded] = useState(defaultOpen)
  return (
    <section className="border-b border-white/10">
      <button
        type="button"
        className="hover:bg-white/5 flex h-9 w-full items-center gap-2 px-3 text-left text-xs font-bold"
        onClick={() => setExpanded((value) => !value)}
        aria-expanded={expanded}
      >
        <Icon className="text-base-content/60 size-3.5" />
        <span className="flex-1">{title}</span>
        {meta}
        <ChevronDown
          className={cn("text-base-content/60 size-3.5 transition", !expanded && "-rotate-90")}
        />
      </button>
      {expanded && <div className="px-3 pb-3">{children}</div>}
    </section>
  )
}

function commanderName(participant: TableParticipant, decks: DeckSummary[]) {
  return (
    decks.find((deck) => deck.id === participant.deck_id)?.commander_name ?? participant.deck_name
  )
}

function SeatOrderTable({
  participants,
  localParticipant,
  decks,
}: Pick<Props, "participants" | "localParticipant" | "decks">) {
  return (
    <table className="w-full text-[0.7rem]">
      <thead className="text-base-content/50 text-[0.6rem] tracking-wider uppercase">
        <tr>
          <th className="w-5 py-1 text-left font-semibold">#</th>
          <th className="py-1 text-left font-semibold">Player</th>
          <th className="py-1 text-left font-semibold">Commander</th>
          <th className="py-1 text-right font-semibold">Life</th>
        </tr>
      </thead>
      <tbody>
        {participants.map((participant, index) => (
          <tr key={participant.peer_id} className="border-t border-white/5">
            <td className="py-1.5 tabular-nums">{index + 1}</td>
            <td className="max-w-24 truncate py-1.5 font-semibold">
              {participant.player_name}
              {participant.peer_id === localParticipant.peer_id && (
                <span className="text-base-content/50 ml-1 font-normal">(you)</span>
              )}
            </td>
            <td className="text-base-content/70 max-w-28 truncate py-1.5">
              {commanderName(participant, decks) ?? "—"}
            </td>
            <td className="py-1.5 text-right font-bold tabular-nums">{participant.life}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
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
    recognizer,
    onInvite,
    inviteCopied,
    onChooseDeck,
    onRandomizeSeats,
    onEndGame,
  } = props

  return (
    <>
      <PanelSection
        title="Setup"
        icon={Gamepad2}
        meta={
          <span className="text-base-content/60 flex items-center gap-1 text-[0.65rem] font-semibold">
            <Users className="size-3" /> {participants.length}/{maxPlayers}
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
            playerName={local.player_name}
            decks={playerDecks}
            selectedDeckId={local.deck_id}
            onChoose={onChooseDeck}
            align="start"
          >
            <button type="button" className="btn btn-outline btn-sm w-full text-xs">
              <Layers className="size-3.5" />
              <span className="truncate">
                {commanderName(local, playerDecks) ?? "Select your commander"}
              </span>
            </button>
          </CommanderPicker>
        </div>

        <h3 className="text-base-content/50 mt-4 mb-1 text-[0.6rem] font-bold tracking-wider uppercase">
          Turn order
        </h3>
        <SeatOrderTable participants={participants} localParticipant={local} decks={decks} />
        <p className="text-base-content/50 mt-1 text-[0.65rem]">
          Seats are recorded in this order when the game ends.
        </p>

        <div className="mt-3 grid gap-1.5">
          <button
            type="button"
            className="btn btn-primary btn-sm w-full text-xs"
            onClick={onRandomizeSeats}
            disabled={participants.length < 2}
          >
            <Shuffle className="size-3.5" /> Randomize turn order
          </button>
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
          runs in your browser and lists its top five; press <kbd className="kbd kbd-xs">1</kbd>–
          <kbd className="kbd kbd-xs">5</kbd> to confirm one or <kbd className="kbd kbd-xs">/</kbd>{" "}
          to search by name, set code or collector number. Confirmed cards go to the Log at every
          seat.
        </p>
        <p className="text-base-content/50 mt-2 text-xs">{describeRecognizer(recognizer)}</p>
      </PanelSection>

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
        </dl>
      </PanelSection>
    </>
  )
}

function DecksTab({ playerDecks, localParticipant: local, onChooseDeck }: Props) {
  return (
    <PanelSection title="Your commanders" icon={Layers}>
      {playerDecks.length === 0 ? (
        <p className="text-base-content/65 text-xs">
          No decks are recorded for you yet.{" "}
          <Link to="/decks" search={{ scope: "mine" }} className="link">
            Add one
          </Link>
          .
        </p>
      ) : (
        <ul className="grid gap-1">
          {playerDecks.map((deck) => {
            const selected = deck.id === local.deck_id
            return (
              <li key={deck.id}>
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
                    <span className="block truncate font-semibold">{deck.commander_name}</span>
                    <span className="text-base-content/55 block truncate">{deck.name}</span>
                  </span>
                  {selected && <Check className="text-primary size-3.5" />}
                </button>
              </li>
            )
          })}
        </ul>
      )}
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
              <span>{event.text}</span>
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
  const Content = tab === "table" ? TableTab : tab === "decks" ? DecksTab : LogTab

  return (
    <div className="bg-base-100 text-base-content flex max-h-[45dvh] flex-col border-t border-white/10 lg:max-h-none lg:flex-row lg:border-t-0 lg:border-l">
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
      </nav>
      {open && (
        <div className="bg-base-200 min-h-0 flex-1 overflow-y-auto border-t border-white/10 lg:w-72 lg:border-t-0 lg:border-l">
          {props.error && (
            <div
              role="alert"
              className="bg-error/20 text-error border-b border-white/10 px-3 py-2 text-xs"
            >
              {props.error}
            </div>
          )}
          <Content {...props} />
        </div>
      )}
    </div>
  )
}
