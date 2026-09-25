import { Link } from "@tanstack/react-router"
import {
  Check,
  Copy,
  DoorOpen,
  Gamepad2,
  Layers,
  Play,
  ScanSearch,
  Shuffle,
  Undo2,
  Users,
  Wifi,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { commanderNames, type DeckSummary } from "@/features/decks/decks"
import { DeckCommanders } from "@/features/decks/deck-commanders"
import {
  GAME_FORMATS,
  formatLabel,
  isGameFormat,
  type GameFormat,
} from "@/features/games/game-format"
import { cn } from "@/lib/cn"
import { CommanderHover } from "./card-hover"
import { CommanderPicker } from "./commander-picker"
import { DeckChooserButton } from "./deck-chooser-button"
import type { TimerSample } from "./game-timer"
import { PanelSection } from "./panel-section"
import type { RecognizerState } from "./recognition/use-recognizer"
import { RevealControl } from "./reveal-control"
import type { TableParticipant } from "./room-types"
import { SeatOrderTable } from "./seat-order-table"
import { describeIceServers, describeRecognizer } from "./side-panel-labels"
import { TableRolls, type RollRequest } from "./table-rolls"
import { Choice } from "./table-settings"
import { TimerBadge, TimerToggle } from "./table-timer"
import { nextActiveSeat, unpassTarget, type TurnState } from "./turns"

export interface TableTabProps {
  mode?: GameFormat
  onModeChange?: (mode: GameFormat) => void
  onMoveSeat?: (peerId: string, delta: -1 | 1) => void
  spectating?: boolean
  isOwner?: boolean
  participants: TableParticipant[]
  localParticipant: TableParticipant
  maxPlayers: number
  playerDecks: DeckSummary[]
  /** Every deck at the table, so remote seats' commanders can be named from their deck_id. */
  decks: DeckSummary[]
  status: string
  error: string | null
  connectedPeers: number
  connectionStates: Record<string, RTCPeerConnectionState>
  iceServers: RTCIceServer[]
  recognizer: RecognizerState
  onInvite: () => void
  inviteCopied: boolean
  onChooseDeck: (deckId: number) => void
  onStartGame: (randomize: boolean) => void
  shuffleVersion: number
  turns: TurnState
  timer: TimerSample | null
  onPassTurn: () => void
  onUnpassTurn: () => void
  onAdjustTurn: (playerId: number, delta: -1 | 1) => void
  onRoll: (request: RollRequest) => void
  onEndGame: () => void
  onChangeTimer: (action: "pause" | "resume") => void
  /** Private reveal: who the local camera is currently shown to, if anyone. */
  reveal: {
    target: string | null
    busy: boolean
    onChange: (target: string | null) => Promise<void>
  }
}

function RecognizerBadge({ state }: { state: RecognizerState }) {
  const tone =
    state.status === "ready"
      ? "badge-success"
      : state.status === "failed"
        ? "badge-error"
        : state.status === "unavailable" || state.status === "idle"
          ? "badge-ghost"
          : "badge-warning"
  const label =
    state.status === "ready"
      ? "ready"
      : state.status === "failed"
        ? "failed"
        : state.status === "unavailable"
          ? "off"
          : state.status === "idle"
            ? "not loaded"
            : "loading"
  return <span className={cn("badge badge-xs", tone)}>{label}</span>
}

function commanderName(participant: TableParticipant, decks: DeckSummary[]) {
  const deck = decks.find((deck) => deck.id === participant.deck_id)
  return deck ? commanderNames(deck) : participant.deck_name
}

function CommanderButton(props: TableTabProps) {
  const { localParticipant: local, playerDecks } = props
  const selectedDeck = playerDecks.find((deck) => deck.id === local.deck_id)
  return (
    <CommanderPicker
      playerId={local.player_id}
      playerName={local.player_name}
      decks={playerDecks}
      selectedDeckId={local.deck_id}
      onChoose={props.onChooseDeck}
      align="start"
    >
      <Button type="button" variant="outline" size="sm" className="h-auto min-h-8 w-full py-1.5">
        <Layers className="size-3.5" />
        <CommanderHover deck={selectedDeck}>
          <span className="min-w-0 flex-1 whitespace-normal">
            {selectedDeck ? (
              <DeckCommanders deck={selectedDeck} />
            ) : (
              (commanderName(local, playerDecks) ?? "Select your commander")
            )}
          </span>
        </CommanderHover>
      </Button>
    </CommanderPicker>
  )
}

/** Mode choice and start buttons before the match; pass turn once it is running. */
function MatchControls(props: TableTabProps) {
  const { participants, isOwner, mode } = props
  const started = props.timer?.state.started_at != null
  const enoughSeats = participants.filter((participant) => !participant.departed).length >= 2
  return (
    <>
      {(started || isOwner === false) && (
        <p className="text-xs text-base-content/70">{formatLabel(mode ?? "commander")}</p>
      )}
      {!started && isOwner !== false && (
        <>
          <Choice
            label="Game mode"
            value={mode ?? "commander"}
            options={GAME_FORMATS}
            onChange={(value) => {
              if (isGameFormat(value)) props.onModeChange?.(value)
            }}
          />
          {mode === "two_headed_giant" && (
            <p className="text-[0.65rem] text-base-content/60">
              Two-Headed Giant Commander · 60 shared life. Adjacent seats form teams; move players
              above to pair them. Randomization keeps pairs together.
            </p>
          )}
          {mode === "five_star" && (
            <p className="text-[0.65rem] text-base-content/60">
              Exactly 5 players. Neighbours can't be attacked until your other two opponents are
              eliminated.
            </p>
          )}
          <div className="grid grid-cols-2 gap-1.5">
            <Button
              type="button"
              size="sm"
              onClick={() => props.onStartGame(false)}
              disabled={!enoughSeats}
              title="Start with the turn order shown above"
            >
              <Play className="size-3.5" /> Start match
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => props.onStartGame(true)}
              disabled={!enoughSeats}
              title={
                mode === "two_headed_giant"
                  ? "Shuffle the teams, then start"
                  : "Shuffle the turn order, then start"
              }
            >
              <Shuffle className="size-3.5" /> Randomize
            </Button>
          </div>
        </>
      )}
      {started && <TurnButtons {...props} />}
    </>
  )
}

function TurnButtons(props: TableTabProps) {
  const { participants, mode, turns } = props
  const previousId = unpassTarget(turns)
  const previous = participants.find((participant) => participant.player_id === previousId)
  return (
    <div className="flex gap-1.5">
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="flex-1 whitespace-nowrap"
        onClick={props.onPassTurn}
        disabled={turns.active_player_id === null}
        title={`Next: ${nextActiveSeat(participants, turns.active_player_id, mode)?.player_name ?? "No eligible players"}`}
      >
        Pass turn <kbd className="kbd kbd-xs">Space</kbd>
      </Button>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="whitespace-nowrap"
        onClick={props.onUnpassTurn}
        disabled={previousId === null}
        aria-label="Un-pass turn"
        title={
          previous
            ? `Un-pass turn: back to ${previous.player_name}`
            : "Un-pass turn: nothing to undo"
        }
      >
        <Undo2 className="size-3.5" aria-hidden /> <kbd className="kbd kbd-xs">⇧ Space</kbd>
      </Button>
    </div>
  )
}

function SetupSection(props: TableTabProps) {
  const { participants, localParticipant: local } = props
  return (
    <PanelSection
      title="Setup"
      icon={Gamepad2}
      meta={
        <span className="text-base-content/60 flex items-center gap-2 text-[0.65rem] font-semibold">
          <TimerBadge sample={props.timer} />
          <span className="flex items-center gap-1">
            <Users className="size-3" /> {participants.length}/{props.maxPlayers}
          </span>
        </span>
      }
    >
      <div className="grid gap-1.5">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="w-full"
          onClick={props.onInvite}
        >
          {props.inviteCopied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          {props.inviteCopied ? "Invite link copied" : "Invite players"}
        </Button>
        <div className="flex gap-1.5">
          <div className="min-w-0 flex-1">
            <CommanderButton {...props} />
          </div>
          {props.timer?.state.started_at == null && (
            <DeckChooserButton onChooseDeck={props.onChooseDeck} />
          )}
        </div>
      </div>

      <h3 className="text-base-content/50 mt-4 mb-1 text-[0.6rem] font-bold tracking-wider uppercase">
        Turn order
      </h3>
      <SeatOrderTable
        mode={props.mode}
        onMoveSeat={props.onMoveSeat}
        participants={participants}
        localParticipant={local}
        decks={props.decks}
        shuffleVersion={props.shuffleVersion}
        turns={props.turns}
        timer={props.timer}
        onAdjustTurn={props.onAdjustTurn}
        readOnly={props.isOwner === false}
      />
      <p className="text-base-content/50 mt-1 text-[0.65rem]">
        The room owner reorders seats with the arrows
        {props.mode !== "commander" ? " until the match starts" : ""}. Once playing, eliminate or
        restore a player from their seat menu; out players skip turns but keep their recorded seat.
      </p>

      <div className="mt-3 grid gap-1.5">
        <MatchControls {...props} />
        {props.isOwner !== false && (
          <TimerToggle sample={props.timer} onChange={props.onChangeTimer} />
        )}
        <RevealControl
          participants={participants}
          peerId={local.peer_id}
          target={props.reveal.target}
          busy={props.reveal.busy}
          onChange={props.reveal.onChange}
        />
        {props.isOwner !== false && (
          <Button
            type="button"
            variant="destructive"
            size="sm"
            className="w-full"
            onClick={props.onEndGame}
            disabled={participants.length < 2}
          >
            <DoorOpen className="size-3.5" /> End game
          </Button>
        )}
        <Button asChild variant="ghost" size="sm" className="w-full">
          <Link to="/games">Leave table</Link>
        </Button>
      </div>
    </PanelSection>
  )
}

function IdentifyCardsSection({ recognizer }: { recognizer: RecognizerState }) {
  return (
    <PanelSection
      title="Identify cards"
      icon={ScanSearch}
      defaultOpen={false}
      meta={<RecognizerBadge state={recognizer} />}
    >
      <p className="text-base-content/70 text-xs leading-relaxed">
        Click a card on any board. The camera owner returns a native 640 px crop, the recognizer
        runs in your browser, and a clear match opens the card with its rules text and lands in that
        board's tray (the tab at the bottom of the video) at every seat. Say "Wrong card?" on the
        preview, or Shift+click, to pick from its top five instead: press{" "}
        <kbd className="kbd kbd-xs">1</kbd>–<kbd className="kbd kbd-xs">5</kbd> or{" "}
        <kbd className="kbd kbd-xs">/</kbd> to search by name, set code or collector number. The
        Cards tab lists everything identified at the table.
      </p>
      <p className="text-base-content/50 mt-2 text-xs">{describeRecognizer(recognizer)}</p>
    </PanelSection>
  )
}

function ConnectionSection(props: TableTabProps) {
  const { error, status, maxPlayers } = props
  return (
    <PanelSection title="Connection" icon={Wifi} defaultOpen={false}>
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
        <dt className="text-base-content/50">Status</dt>
        <dd className={cn(error && "text-error")}>{error ?? status}</dd>
        <dt className="text-base-content/50">Peers</dt>
        <dd>
          {props.connectedPeers} of {Math.max(0, props.participants.length - 1)} streaming
        </dd>
        <dt className="text-base-content/50">Video</dt>
        <dd>1080p mesh, up to {maxPlayers} players</dd>
        <dt className="text-base-content/50">ICE</dt>
        <dd>{describeIceServers(props.iceServers)}</dd>
      </dl>
      {Object.values(props.connectionStates).some((state) => state === "failed") && (
        <p className="text-warning mt-2 text-xs leading-relaxed">
          A peer couldn't be reached directly. Players on different networks usually need a TURN
          relay: set <code>CLOUDFLARE_TURN_KEY_ID</code> and <code>CLOUDFLARE_TURN_API_TOKEN</code>{" "}
          or the <code>WEBRTC_TURN_*</code> variables on the server (see docs/webcam-table.md).
        </p>
      )}
    </PanelSection>
  )
}

export function TableTab(props: TableTabProps) {
  if (props.spectating)
    return (
      <PanelSection title="Spectating" icon={Users}>
        <p className="mb-3 text-xs text-base-content/70">
          The game is in progress. Seats are reserved for returning players.
        </p>
        <TimerBadge sample={props.timer} />
        <SeatOrderTable {...props} readOnly />
      </PanelSection>
    )

  return (
    <>
      <SetupSection {...props} />
      <IdentifyCardsSection recognizer={props.recognizer} />
      <TableRolls onRoll={props.onRoll} />
      <ConnectionSection {...props} />
    </>
  )
}
