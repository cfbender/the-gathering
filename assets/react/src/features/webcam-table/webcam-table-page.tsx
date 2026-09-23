import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import { getPlayers } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"
import { ActiveBoard, CameraTile, OpenSeat, capturePoint } from "./board"
import { BoardCardTray } from "./board-cards"
import { CardPreview } from "./card-preview"
import { CardSuggestions, isClear, type Recognition } from "./card-suggestions"
import { FinishGame } from "./finish-game"
import { canViewBoard } from "./media-policy"
import type { GalleryArt } from "./recognition/pipeline"
import { decodeImage, useRecognizer, type RecognizerState } from "./recognition/use-recognizer"
import { RevealControl } from "./reveal-control"
import { SeatBar, TileCommanderRow } from "./seat-bar"
import { SeatCounterControls } from "./seat-counter-controls"
import { SidePanel, type PanelTab } from "./side-panel"
import { HotkeyHelp, useTableHotkeys } from "./table-hotkeys"
import { RailResizeHandle, TableSettings, useTablePreferences } from "./table-preferences"
import {
  useWebcamRoom,
  type BoardCard,
  type CapturedCard,
  type IdentifiedCard,
  type TableParticipant,
} from "./use-webcam-room"

const MAX_PLAYERS = 10

interface Props {
  roomId: string
}

interface LiveRoomProps extends Props {
  playerId: number
  playerName: string
  decks: DeckSummary[]
}

function streamFor(
  participant: TableParticipant,
  peerId: string,
  localStream: MediaStream | null,
  streams: Record<string, MediaStream>,
) {
  return participant.peer_id === peerId ? (localStream ?? undefined) : streams[participant.peer_id]
}

/** Which board fills the stage: the pinned/selected one, else the newest remote joiner, else you.
 * Falls back to your own board when the selected player leaves. */
function useActiveBoard(participants: TableParticipant[], localPeerId: string) {
  const [selectedPeerId, setSelectedPeerId] = useState(localPeerId)
  const [pinned, setPinned] = useState(false)
  const knownPeers = useRef(new Set<string>([localPeerId]))

  useEffect(() => {
    const present = new Set(participants.map((participant) => participant.peer_id))
    const newcomers = participants.filter(
      (participant) => !knownPeers.current.has(participant.peer_id),
    )
    knownPeers.current = new Set([localPeerId, ...present])

    if (!present.has(selectedPeerId) && selectedPeerId !== localPeerId) {
      setSelectedPeerId(localPeerId)
      setPinned(false)
    } else if (!pinned && newcomers.length > 0) {
      const newest = newcomers[newcomers.length - 1]
      if (newest && newest.peer_id !== localPeerId) setSelectedPeerId(newest.peer_id)
    }
  }, [localPeerId, participants, pinned, selectedPeerId])

  return {
    selectedPeerId,
    pinned,
    select: (peerId: string) => {
      setSelectedPeerId(peerId)
      setPinned(true)
    },
    togglePin: () => setPinned((value) => !value),
  }
}

/** Why a click did not get recognized, for the suggestion panel footer. */
function skippedReason(state: RecognizerState): string {
  switch (state.status) {
    case "unavailable":
      return "not installed on this server"
    case "checking":
    case "loading":
      return "still loading"
    case "failed":
      return `failed: ${state.message}`
    case "ready":
      return "unavailable"
  }
}

/** Runs the recognizer on every new capture: decode the owner's crop, identify at the click,
 * and hold the outcome next to the capture it belongs to. The outcome is only reported while
 * that same capture is current, so a new click never sees the previous click's answer. */
function useRecognition(capture: CapturedCard | null) {
  const recognizer = useRecognizer()
  const [outcome, setOutcome] = useState<{ capture: CapturedCard; recognition: Recognition }>()

  useEffect(() => {
    if (!capture) return
    if (!recognizer.ready) {
      setOutcome({
        capture,
        recognition: { status: "skipped", reason: skippedReason(recognizer.state) },
      })
      return
    }
    let stale = false
    decodeImage(capture.image)
      .then((image) => recognizer.identify(image, capture.clickX, capture.clickY))
      .then((result) => {
        if (!stale) setOutcome({ capture, recognition: { status: "done", result } })
      })
      .catch((error: unknown) => {
        if (stale) return
        const message = error instanceof Error ? error.message : String(error)
        setOutcome({
          capture,
          recognition: {
            status: "skipped",
            reason: message.startsWith("no result") ? "timed out" : message,
          },
        })
      })
    return () => {
      stale = true
    }
    // Re-run for a new capture only; the recognizer becoming ready later does not re-identify.
  }, [capture])

  const recognition: Recognition =
    outcome && outcome.capture === capture ? outcome.recognition : { status: "identifying" }
  return { recognizer, recognition }
}

/** What the card overlay on the active board is showing: an identified board entry (offering
 * "Wrong card?" while its capture is still current), or a printing from the gallery search. */
type Preview =
  | { kind: "entry"; entry: BoardCard; correctable: boolean }
  | { kind: "art"; card: IdentifiedCard }

function toCard(art: GalleryArt): IdentifiedCard {
  return { id: art.id, name: art.name, set: art.set, collector_number: art.collector_number }
}

function LiveRoom({ roomId, playerId, playerName, decks }: LiveRoomProps) {
  const room = useWebcamRoom(roomId, playerId, null)
  const { recognizer, recognition } = useRecognition(room.capture)
  const [preview, setPreview] = useState<Preview | null>(null)
  /** The picker is open by request ("Wrong card?"), replacing this entry if one is named. */
  const [picker, setPicker] = useState<{ replacing: string | null } | null>(null)
  const autoChosen = useRef<CapturedCard | null>(null)
  const [inviteCopied, setInviteCopied] = useState(false)
  const [finishOpen, setFinishOpen] = useState(false)
  const [panelOpen, setPanelOpen] = useState(true)
  const [panelTab, setPanelTab] = useState<PanelTab>("table")
  const [helpOpen, setHelpOpen] = useState(false)
  const preferences = useTablePreferences(playerId)
  const playedAt = useRef(new Date())
  const board = useActiveBoard(room.participants, room.peerId)

  const localParticipant: TableParticipant = room.participants.find(
    (participant) => participant.peer_id === room.peerId,
  ) ?? {
    peer_id: room.peerId,
    player_id: playerId,
    player_name: playerName,
    life: room.life,
    ...room.counters,
    camera_off: room.cameraOff,
    joined_at: Number.MAX_SAFE_INTEGER,
  }
  const seated = room.participants.some((participant) => participant.peer_id === room.peerId)
    ? room.participants
    : [localParticipant, ...room.participants]
  const activeParticipant =
    seated.find((participant) => participant.peer_id === board.selectedPeerId) ?? localParticipant
  const revealFor = (participant: TableParticipant) => ({
    hiddenLabel: canViewBoard(participant.peer_id, room.peerId, participant.reveal_to)
      ? undefined
      : `Revealing to ${seated.find((seat) => seat.peer_id === participant.reveal_to)?.player_name ?? "another player"}`,
    revealBadge:
      participant.reveal_to === room.peerId
        ? `${participant.player_name} is revealing to you`
        : undefined,
  })
  const decksFor = (participant: TableParticipant) =>
    decks.filter((deck) => deck.player_id === participant.player_id)
  const chooseFor = (participant: TableParticipant) => (deckId: number) =>
    room.suggestDeck(participant.peer_id, deckId)
  const captureOwner = room.capture
    ? seated.find((participant) => participant.peer_id === room.capture?.peerId)
    : undefined
  const suggestions = captureOwner ? decksFor(captureOwner).slice(0, 5) : []
  const candidates = recognition.status === "done" ? recognition.result.candidates : []
  // The picker is for the cases a human has to settle: no recognizer, a near-tie, a
  // Shift+click asking to choose, or "Wrong card?" on a result. A clear answer to a plain
  // click is recorded without it and shown as the card itself.
  const needsChoice =
    room.capture !== null &&
    (room.capture.inspect ||
      recognition.status === "skipped" ||
      (recognition.status === "done" && !isClear(candidates)))
  const pickerOpen =
    room.capture !== null &&
    captureOwner !== undefined &&
    !preview &&
    !helpOpen &&
    !finishOpen &&
    (needsChoice || !!picker)

  /** Adds the card to the owner's board list at every seat and shows it; a card that is one of
   * the owner's commanders also picks that deck when they have not chosen one yet. Replaces the
   * entry being corrected when the picker came from "Wrong card?". */
  const chooseCard = useCallback(
    (art: GalleryArt) => {
      if (!captureOwner) return
      const commanderDeck = decksFor(captureOwner).find(
        (deck) => deck.commander_name.toLowerCase() === art.name.toLowerCase(),
      )
      if (commanderDeck && !captureOwner.deck_id)
        room.suggestDeck(captureOwner.peer_id, commanderDeck.id)
      if (picker?.replacing) room.removeCard(picker.replacing)
      const entry = room.announceCard(captureOwner.peer_id, playerName, toCard(art))
      setPicker(null)
      setPreview({ kind: "entry", entry, correctable: true })
    },
    // decksFor closes over `decks`, which is stable for the room's lifetime
    [captureOwner, decks, picker, playerName, room],
  )

  // A new click replaces whatever the last one left on screen.
  useEffect(() => {
    setPreview(null)
    setPicker(null)
  }, [room.capture])

  useEffect(() => {
    const top = candidates[0]
    if (!top || !room.capture || room.capture.inspect || !isClear(candidates)) return
    if (autoChosen.current === room.capture) return
    autoChosen.current = room.capture
    chooseCard(top)
  }, [candidates, chooseCard, room.capture])

  const closePreview = useCallback(() => {
    setPreview(null)
    room.dismissCapture()
  }, [room])

  const dismissPicker = () => {
    setPicker(null)
    room.dismissCapture()
  }

  useTableHotkeys(preferences.hotkeys, pickerOpen, (action) => {
    switch (action) {
      case "gainLife":
        return room.changeLife(1)
      case "loseLife":
        return room.changeLife(-1)
      case "camera":
        return room.toggleCamera()
      case "panel":
        return setPanelOpen((open) => !open)
      case "help":
        return setHelpOpen(true)
      case "dismiss":
        return dismissPicker()
      case "previous":
      case "next": {
        const index = seated.findIndex(
          (participant) => participant.peer_id === activeParticipant.peer_id,
        )
        const next = seated[(index + (action === "next" ? 1 : -1) + seated.length) % seated.length]
        if (next) board.select(next.peer_id)
        return
      }
      default:
        setPanelTab(action)
        setPanelOpen(true)
    }
  })

  useEffect(() => {
    function choose(event: KeyboardEvent) {
      if (!pickerOpen || !room.capture || event.key < "1" || event.key > "5") return
      if (event.target instanceof HTMLElement && event.target.matches("input, textarea, select"))
        return
      const index = Number(event.key) - 1
      const art = candidates[index]
      if (art) return chooseCard(art)
      const deck = suggestions[index]
      if (deck && recognition.status === "skipped") room.suggestDeck(room.capture.peerId, deck.id)
    }
    window.addEventListener("keydown", choose)
    return () => window.removeEventListener("keydown", choose)
  }, [candidates, chooseCard, pickerOpen, recognition.status, room, suggestions])

  useEffect(() => {
    if (!inviteCopied) return
    const timer = window.setTimeout(() => setInviteCopied(false), 2500)
    return () => window.clearTimeout(timer)
  }, [inviteCopied])

  const countersFor = (participant: TableParticipant) => (
    <SeatCounterControls
      participant={participant}
      participants={seated}
      decks={decks}
      local={participant.peer_id === room.peerId}
      monarch={room.monarch?.peer_id === participant.peer_id}
      onAdjust={room.adjustCounter}
      onTakeMonarch={room.takeMonarch}
    />
  )

  const seatBarFor = (participant: TableParticipant, size: "board" | "tile") => (
    <SeatBar
      participant={participant}
      local={participant.peer_id === room.peerId}
      decks={decksFor(participant)}
      size={size}
      onChooseDeck={chooseFor(participant)}
      onChangeLife={room.changeLife}
      onToggleCamera={room.toggleCamera}
      counters={countersFor(participant)}
    />
  )

  return (
    <div
      className="grid h-dvh grid-rows-[auto_minmax(0,1fr)_auto] bg-black text-white lg:grid-cols-[var(--table-camera-width)_0.375rem_minmax(0,1fr)_auto_auto] lg:grid-rows-1"
      style={
        {
          "--table-camera-width": `min(${preferences.camera}px, 24vw)`,
          "--table-panel-width": `min(${preferences.panel}px, 32vw)`,
        } as CSSProperties
      }
    >
      <aside
        className="flex min-h-0 gap-1.5 overflow-x-auto p-1.5 lg:flex-col lg:overflow-x-hidden lg:overflow-y-auto"
        aria-label="Player cameras"
      >
        <RevealControl
          participants={seated}
          peerId={room.peerId}
          target={room.revealTo}
          busy={room.revealBusy}
          onChange={room.changeReveal}
        />
        {seated.map((participant) => (
          <div
            key={participant.peer_id}
            className="w-44 shrink-0 overflow-hidden rounded-sm lg:w-auto"
          >
            <CameraTile
              participant={participant}
              monarch={room.monarch?.peer_id === participant.peer_id}
              {...revealFor(participant)}
              local={participant.peer_id === room.peerId}
              active={participant.peer_id === activeParticipant.peer_id}
              connectionState={room.connectionStates[participant.peer_id]}
              stream={streamFor(participant, room.peerId, room.localStream, room.streams)}
              onActivate={() => board.select(participant.peer_id)}
            />
            {seatBarFor(participant, "tile")}
            <TileCommanderRow
              participant={participant}
              decks={decksFor(participant)}
              onChooseDeck={chooseFor(participant)}
              counters={countersFor(participant)}
            />
          </div>
        ))}
        {Array.from({ length: Math.max(0, MAX_PLAYERS - seated.length) }, (_, index) => (
          <div key={`open-${index}`} className="hidden w-44 shrink-0 lg:block lg:w-auto">
            <OpenSeat />
          </div>
        ))}
      </aside>

      <RailResizeHandle
        rail="camera"
        width={preferences.camera}
        onChange={(width) => preferences.setWidth("camera", width)}
      />

      <section className="relative flex min-h-0 min-w-0 flex-col" aria-label="Active board">
        <div className="relative min-h-0 flex-1">
          <ActiveBoard
            participant={activeParticipant}
            monarch={room.monarch?.peer_id === activeParticipant.peer_id}
            {...revealFor(activeParticipant)}
            local={activeParticipant.peer_id === room.peerId}
            connectionState={room.connectionStates[activeParticipant.peer_id]}
            stream={streamFor(activeParticipant, room.peerId, room.localStream, room.streams)}
            pinned={board.pinned}
            onTogglePin={board.togglePin}
            onInspect={(event) => {
              const point = capturePoint(event)
              if (point)
                room.requestCapture(activeParticipant.peer_id, point.x, point.y, event.shiftKey)
            }}
          />
          <BoardCardTray
            participant={activeParticipant}
            cards={room.identifiedCards}
            onPreview={(entry) => setPreview({ kind: "entry", entry, correctable: false })}
            onRemove={room.removeCard}
          />
          {room.capture && captureOwner && pickerOpen && (
            <CardSuggestions
              capture={room.capture}
              playerName={captureOwner.player_name}
              recognition={recognition}
              deckSuggestions={suggestions}
              gallerySearchable={recognizer.ready}
              onChooseCard={chooseCard}
              onChooseDeck={(deckId) => room.suggestDeck(captureOwner.peer_id, deckId)}
              onSearch={recognizer.search}
              onDismiss={dismissPicker}
            />
          )}
          {preview?.kind === "entry" && (
            <CardPreview
              card={preview.entry.card}
              ownerName={
                seated.find((participant) => participant.peer_id === preview.entry.ownerPeerId)
                  ?.player_name
              }
              onWrongCard={
                preview.correctable && room.capture
                  ? () => {
                      setPicker({ replacing: preview.entry.id })
                      setPreview(null)
                    }
                  : undefined
              }
              onRemove={() => {
                room.removeCard(preview.entry.id)
                closePreview()
              }}
              onClose={closePreview}
            />
          )}
          {preview?.kind === "art" && <CardPreview card={preview.card} onClose={closePreview} />}
        </div>
        {seatBarFor(activeParticipant, "board")}
      </section>

      {panelOpen ? (
        <RailResizeHandle
          rail="panel"
          width={preferences.panel}
          onChange={(width) => preferences.setWidth("panel", width)}
        />
      ) : (
        <div className="hidden lg:block" />
      )}

      <SidePanel
        onHelp={() => setHelpOpen(true)}
        settings={
          <TableSettings
            hotkeys={preferences.hotkeys}
            onHotkeysChange={preferences.setHotkeys}
            onResetWidths={preferences.resetWidths}
            onHelp={() => setHelpOpen(true)}
          />
        }
        open={panelOpen}
        tab={panelTab}
        onOpenChange={setPanelOpen}
        onTabChange={setPanelTab}
        participants={seated}
        localParticipant={localParticipant}
        maxPlayers={MAX_PLAYERS}
        playerDecks={decksFor(localParticipant)}
        decks={decks}
        events={room.events}
        status={room.status}
        error={room.error}
        connectedPeers={Object.keys(room.streams).length}
        connectionStates={room.connectionStates}
        iceServers={room.iceServers}
        recognizer={recognizer.state}
        identifiedCards={room.identifiedCards}
        gallerySearchable={recognizer.ready}
        onSearch={recognizer.search}
        onPreviewCard={(entry) => setPreview({ kind: "entry", entry, correctable: false })}
        onPreviewArt={(art) => setPreview({ kind: "art", card: toCard(art) })}
        onRemoveCard={room.removeCard}
        inviteCopied={inviteCopied}
        onInvite={() => {
          void navigator.clipboard.writeText(window.location.href)
          setInviteCopied(true)
        }}
        onChooseDeck={room.chooseDeck}
        onRandomizeSeats={room.randomizeSeats}
        onEndGame={() => setFinishOpen(true)}
      />

      <HotkeyHelp open={helpOpen} onOpenChange={setHelpOpen} />

      {finishOpen && (
        <FinishGame
          participants={room.participants}
          playedAt={playedAt.current}
          onOpenChange={setFinishOpen}
        />
      )}
    </div>
  )
}

export function WebcamTablePage({ roomId }: Props) {
  const session = useCurrentUser()
  const playersQuery = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const decksQuery = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  const player = playersQuery.data?.find((candidate) => candidate.user_id === session.data?.id)

  if (playersQuery.isPending || decksQuery.isPending) {
    return (
      <div className="grid min-h-dvh place-items-center bg-black">
        <span className="loading loading-spinner loading-lg" aria-label="Loading table" />
      </div>
    )
  }

  if (!player) {
    return (
      <div className="grid min-h-dvh place-items-center bg-black p-6">
        <div role="alert" className="alert alert-warning max-w-xl">
          <div>
            <h1 className="font-bold">No linked player</h1>
            <p>Your account must be linked to a player before joining a table.</p>
          </div>
          <Link to="/games" className="btn btn-sm">
            Back to games
          </Link>
        </div>
      </div>
    )
  }

  return (
    <LiveRoom
      roomId={roomId}
      playerId={player.id}
      playerName={player.name}
      decks={decksQuery.data ?? []}
    />
  )
}
