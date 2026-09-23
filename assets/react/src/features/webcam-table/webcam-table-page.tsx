import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogClose,
} from "@/components/ui/dialog"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import { getPlayers } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { ActiveBoard, CameraTile, OpenSeat, capturePoint } from "./board"
import { BoardCardTray } from "./board-cards"
import { CardPreview } from "./card-preview"
import { CardSuggestions, isClear, type Recognition } from "./card-suggestions"
import { FinishGame } from "./finish-game"
import { LifeControl } from "./life-control"
import { canViewBoard } from "./media-policy"
import type { GameTimerState } from "./game-timer"
import type { GalleryArt } from "./recognition/pipeline"
import { decodeImage, useRecognizer } from "./recognition/use-recognizer"
import { RevealControl } from "./reveal-control"
import { SeatBar } from "./seat-bar"
import { SeatCounterControls } from "./seat-counter-controls"
import { SidePanel, type PanelTab } from "./side-panel"
import { HotkeyHelp, useTableHotkeys } from "./table-hotkeys"
import { RailResizeHandle, useTablePreferences } from "./table-preferences"
import { TableSettings } from "./table-settings"
import { useCorrectionUpload } from "./use-correction-upload"
import { useTurnSound } from "./use-turn-sound"
import { useVideoStats, VideoStatsOverlay } from "./video-stats"
import { describeRoll } from "./table-rolls"
import {
  useWebcamRoom,
  type BoardCard,
  type CapturedCard,
  type IdentifiedCard,
  type TableParticipant,
} from "./use-webcam-room"
import { MAX_PLAYERS } from "./rooms"

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

/** Runs the recognizer on every new capture: decode the owner's crop, identify at the click,
 * and hold the outcome next to the capture it belongs to. The outcome is only reported while
 * that same capture is current, so a new click never sees the previous click's answer. */
function useRecognition(capture: CapturedCard | null) {
  const recognizer = useRecognizer()
  const [outcome, setOutcome] = useState<{ capture: CapturedCard; recognition: Recognition }>()

  useEffect(() => {
    if (!capture) return
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
  }, [capture, recognizer.identify])

  const recognition: Recognition =
    outcome && outcome.capture === capture
      ? outcome.recognition
      : { status: "identifying", loading: !recognizer.ready }
  return { recognizer, recognition }
}

/** What the card overlay on the active board is showing: an identified board entry (offering
 * "Wrong card?" while its capture is still current), or a printing from the gallery search. */
type Preview =
  | { kind: "entry"; entry: BoardCard; shown: IdentifiedCard; correctable: boolean }
  | { kind: "art"; card: IdentifiedCard }

function toCard(art: GalleryArt): IdentifiedCard {
  return { id: art.id, name: art.name, set: art.set, collector_number: art.collector_number }
}

function LiveRoom({ roomId, playerId, playerName, decks }: LiveRoomProps) {
  const preferences = useTablePreferences(playerId)
  const room = useWebcamRoom(
    roomId,
    playerId,
    null,
    preferences.deviceId,
    preferences.quality,
    preferences.cameraEnabled,
  )
  const toggleCamera = () => {
    preferences.update({ cameraEnabled: room.cameraOff })
    room.toggleCamera()
  }
  const videoStats = useVideoStats(preferences.stats, room.getPeerStats)
  useTurnSound(preferences.turnSound, room.turns.active_player_id, playerId)
  const { recognizer, recognition } = useRecognition(room.capture)
  const corrections = useCorrectionUpload()
  const [preview, setPreview] = useState<Preview | null>(null)
  /** The picker is open by request ("Wrong card?"), replacing this entry if one is named. */
  const [picker, setPicker] = useState<{ replacing: string | null } | null>(null)
  const autoChosen = useRef<CapturedCard | null>(null)
  const [inviteCopied, setInviteCopied] = useState(false)
  const [finishOpen, setFinishOpen] = useState(false)
  const [revealOpen, setRevealOpen] = useState(false)
  const [resultTimer, setResultTimer] = useState<GameTimerState | null>(null)
  const [panelOpen, setPanelOpen] = useState(true)
  const [panelTab, setPanelTab] = useState<PanelTab>("table")
  const [helpOpen, setHelpOpen] = useState(false)
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
    eliminated: false,
    joined_at: Number.MAX_SAFE_INTEGER,
  }
  const seated =
    room.spectating || room.participants.some((participant) => participant.peer_id === room.peerId)
      ? room.participants
      : [localParticipant, ...room.participants]
  const activeParticipant =
    (preferences.followTurn &&
      seated.find((participant) => participant.player_id === room.turns.active_player_id)) ||
    seated.find((participant) => participant.peer_id === board.selectedPeerId) ||
    (room.spectating && seated[0]) ||
    localParticipant
  const selectBoard = (peerId: string) => {
    preferences.update({ followTurn: false })
    board.select(peerId)
  }
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
  const captureOwner = room.capture
    ? seated.find((participant) => participant.peer_id === room.capture?.peerId)
    : undefined
  // Only a seat's owner may pick its commander, so deck shortcuts appear on your own clicks only.
  const captureIsLocal = captureOwner?.peer_id === room.peerId
  const suggestions = captureOwner && captureIsLocal ? decksFor(captureOwner).slice(0, 5) : []
  const chooseDeckForCapture = (deckId: number) => {
    if (!captureIsLocal) return
    room.chooseDeck(deckId)
    dismissPicker()
  }
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
    (art: GalleryArt, explicit = true) => {
      if (!captureOwner) return
      corrections.save(
        room.capture,
        recognition.status === "done" ? recognition.result : undefined,
        art.id,
        "version" in recognizer.state ? recognizer.state.version : "unavailable",
        explicit,
      )
      const commanderDeck = decksFor(captureOwner).find(
        (deck) => deck.commander_name.toLowerCase() === art.name.toLowerCase(),
      )
      if (commanderDeck && !captureOwner.deck_id && captureOwner.peer_id === room.peerId)
        room.chooseDeck(commanderDeck.id)
      if (picker?.replacing) room.removeCard(picker.replacing)
      const entry = room.announceCard(captureOwner.peer_id, playerName, toCard(art))
      setPicker(null)
      setPreview({ kind: "entry", entry, shown: toCard(art), correctable: true })
    },
    // decksFor closes over `decks`, which is stable for the room's lifetime
    [captureOwner, corrections, decks, picker, playerName, recognition, recognizer.state, room],
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
    chooseCard(top, false)
  }, [candidates, chooseCard, room.capture])

  const closePreview = useCallback(() => {
    setPreview(null)
    room.dismissCapture()
  }, [room])

  const dismissPicker = () => {
    setPicker(null)
    room.dismissCapture()
  }

  useTableHotkeys(preferences.hotkeys && !room.spectating, pickerOpen, (action) => {
    switch (action) {
      case "gainLife":
        return room.changeLife(1)
      case "loseLife":
        return room.changeLife(-1)
      case "gainTenLife":
        return room.changeLife(10)
      case "loseTenLife":
        return room.changeLife(-10)
      case "passTurn":
        if (room.turns.active_player_id !== null) room.passTurn()
        return
      case "gainTax":
      case "loseTax": {
        const deck = decks.find((deck) => deck.id === localParticipant.deck_id)
        if (deck)
          room.adjustCounter(
            { kind: "casts", commander: deck.commander_name },
            action === "gainTax" ? 1 : -1,
          )
        return
      }
      case "camera":
        return toggleCamera()
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
        if (next) selectBoard(next.peer_id)
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
      if (deck && recognition.status === "skipped") chooseDeckForCapture(deck.id)
    }
    window.addEventListener("keydown", choose)
    return () => window.removeEventListener("keydown", choose)
  }, [
    candidates,
    chooseCard,
    chooseDeckForCapture,
    pickerOpen,
    recognition.status,
    room,
    suggestions,
  ])

  useEffect(() => {
    if (!inviteCopied) return
    const timer = window.setTimeout(() => setInviteCopied(false), 2500)
    return () => window.clearTimeout(timer)
  }, [inviteCopied])

  const countersFor = (participant: TableParticipant, onOpenChange: (open: boolean) => void) => (
    <SeatCounterControls
      participant={participant}
      participants={seated}
      decks={decks}
      local={participant.peer_id === room.peerId}
      monarch={room.monarch?.peer_id === participant.peer_id}
      onAdjust={room.adjustCounter}
      onChangeLife={room.changeLife}
      onTakeMonarch={room.takeMonarch}
      onOpenChange={onOpenChange}
    />
  )

  const lifeControlFor = (participant: TableParticipant, size: "board" | "tile") => (
    <LifeControl
      life={participant.life}
      local={participant.peer_id === room.peerId}
      size={size}
      counters={(onOpenChange) => countersFor(participant, onOpenChange)}
      onChangeLife={room.changeLife}
    />
  )
  const isPinned = (participant: TableParticipant) =>
    !preferences.followTurn && board.pinned && participant.peer_id === activeParticipant.peer_id
  const togglePinFor = (participant: TableParticipant) => {
    if (isPinned(participant)) board.togglePin()
    else selectBoard(participant.peer_id)
  }

  const seatBarFor = (participant: TableParticipant, size: "board" | "tile") => (
    <SeatBar
      participant={participant}
      local={participant.peer_id === room.peerId}
      decks={decksFor(participant)}
      size={size}
      onChooseDeck={room.chooseDeck}
      onToggleCamera={toggleCamera}
      onReveal={() => setRevealOpen(true)}
      pinned={isPinned(participant)}
      onTogglePin={() => togglePinFor(participant)}
      onSetEliminated={(eliminated) => room.setEliminated(participant.peer_id, eliminated)}
      canEliminate={!room.spectating && (room.isOwner || participant.peer_id === room.peerId)}
    />
  )

  return (
    <div
      className={cn(
        "grid h-dvh grid-rows-[auto_minmax(0,1fr)_auto] bg-black text-white lg:grid-rows-1",
        preferences.panelLeft
          ? "lg:grid-cols-[auto_auto_minmax(0,1fr)_0.375rem_var(--table-camera-width)]"
          : "lg:grid-cols-[var(--table-camera-width)_0.375rem_minmax(0,1fr)_auto_auto]",
      )}
      style={
        {
          "--table-camera-width": `min(${preferences.camera}px, 24vw)`,
          "--table-panel-width": `min(${preferences.panel}px, 32vw)`,
        } as CSSProperties
      }
    >
      <aside
        className={cn(
          "flex min-h-0 gap-1.5 overflow-x-auto p-1.5 lg:flex-col lg:overflow-x-hidden lg:overflow-y-auto",
          preferences.panelLeft && "lg:order-5",
        )}
        aria-label="Player cameras"
      >
        {seated.map((participant) => (
          <div
            key={participant.peer_id}
            className="w-60 shrink-0 overflow-hidden rounded-sm lg:w-auto"
          >
            <div className="relative">
              <CameraTile
                participant={participant}
                monarch={room.monarch?.peer_id === participant.peer_id}
                {...revealFor(participant)}
                local={participant.peer_id === room.peerId}
                active={participant.peer_id === activeParticipant.peer_id}
                currentTurn={participant.player_id === room.turns.active_player_id}
                connectionState={room.connectionStates[participant.peer_id]}
                stream={streamFor(participant, room.peerId, room.localStream, room.streams)}
                onActivate={() => selectBoard(participant.peer_id)}
                lifeControl={lifeControlFor(participant, "tile")}
              />
              {preferences.stats && (
                <VideoStatsOverlay
                  stats={videoStats[participant.peer_id]}
                  localStream={participant.peer_id === room.peerId ? room.localStream : undefined}
                />
              )}
            </div>
            {seatBarFor(participant, "tile")}
          </div>
        ))}
        {Array.from(
          {
            length:
              room.timer?.state.started_at != null ? 0 : Math.max(0, MAX_PLAYERS - seated.length),
          },
          (_, index) => (
            <div key={`open-${index}`} className="hidden w-44 shrink-0 lg:block lg:w-auto">
              <OpenSeat />
            </div>
          ),
        )}
      </aside>

      <RailResizeHandle
        rail="camera"
        reversed={preferences.panelLeft}
        width={preferences.camera}
        onChange={(width) => preferences.setWidth("camera", width)}
      />

      <section
        className={cn(
          "relative flex min-h-0 min-w-0 flex-col",
          preferences.panelLeft && "lg:order-3",
        )}
        aria-label="Active board"
      >
        {room.spectating && (
          <p
            role="status"
            className="bg-base-200 px-4 py-2 text-sm font-semibold text-base-content"
          >
            Spectating — this game has already started. Your camera is not shared.
            <Link to="/games" className="link ml-3">
              Leave table
            </Link>
          </p>
        )}
        {room.roll && (
          <div
            role="status"
            className="pointer-events-none absolute top-20 left-1/2 z-30 w-max max-w-[90%] -translate-x-1/2 rounded-xl border border-accent/40 bg-base-100/95 px-6 py-4 text-center text-lg font-semibold text-base-content shadow-xl"
          >
            {describeRoll(room.roll)}
          </div>
        )}
        <div className="relative min-h-0 flex-1">
          <ActiveBoard
            participant={activeParticipant}
            monarch={room.monarch?.peer_id === activeParticipant.peer_id}
            {...revealFor(activeParticipant)}
            local={activeParticipant.peer_id === room.peerId}
            currentTurn={activeParticipant.player_id === room.turns.active_player_id}
            connectionState={room.connectionStates[activeParticipant.peer_id]}
            stream={streamFor(activeParticipant, room.peerId, room.localStream, room.streams)}
            lifeControl={lifeControlFor(activeParticipant, "board")}
            pinned={isPinned(activeParticipant)}
            onTogglePin={() => togglePinFor(activeParticipant)}
            onInspect={(event) => {
              const point = capturePoint(event)
              if (point)
                room.requestCapture(activeParticipant.peer_id, point.x, point.y, event.shiftKey)
            }}
          />
          <BoardCardTray
            participant={activeParticipant}
            cards={room.identifiedCards}
            onPreview={(entry) =>
              setPreview({ kind: "entry", entry, shown: entry.card, correctable: false })
            }
            onRemove={room.removeCard}
            onClear={
              activeParticipant.peer_id === localParticipant.peer_id
                ? room.clearOwnCards
                : undefined
            }
          />
          {room.capture && recognition.status === "identifying" && !pickerOpen && !preview && (
            <p
              role="status"
              className="absolute bottom-12 left-1/2 z-10 -translate-x-1/2 rounded-lg bg-base-100/95 px-4 py-2 text-sm text-base-content shadow-lg"
            >
              {recognition.loading ? "Loading card scanner…" : "Identifying card…"}
            </p>
          )}
          {room.capture && captureOwner && pickerOpen && (
            <CardSuggestions
              capture={room.capture}
              playerName={captureOwner.player_name}
              recognition={recognition}
              deckSuggestions={suggestions}
              gallerySearchable
              onChooseCard={chooseCard}
              onChooseDeck={chooseDeckForCapture}
              onSearch={recognizer.search}
              onPrintings={recognizer.printings}
              galleryVersion={"version" in recognizer.state ? recognizer.state.version : undefined}
              onDismiss={dismissPicker}
            />
          )}
          {preview?.kind === "entry" && (
            <CardPreview
              card={preview.shown}
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
          reversed={preferences.panelLeft}
          width={preferences.panel}
          onChange={(width) => preferences.setWidth("panel", width)}
        />
      ) : (
        <div className={cn("hidden lg:block", preferences.panelLeft && "lg:order-2")} />
      )}

      <SidePanel
        spectating={room.spectating}
        isOwner={room.isOwner}
        left={preferences.panelLeft}
        onHelp={() => setHelpOpen(true)}
        settings={
          <TableSettings
            preferences={preferences}
            room={{ ...room, toggleCamera }}
            corrections={corrections}
            recognizer={recognizer.state}
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
        gallerySearchable
        onSearch={recognizer.search}
        onPreviewCard={(entry) =>
          setPreview({ kind: "entry", entry, shown: entry.card, correctable: false })
        }
        onPreviewArt={(art) => setPreview({ kind: "art", card: toCard(art) })}
        onRemoveCard={room.removeCard}
        onClearOwnCards={room.clearOwnCards}
        inviteCopied={inviteCopied}
        onInvite={() => {
          void navigator.clipboard.writeText(window.location.href)
          setInviteCopied(true)
        }}
        onChooseDeck={room.chooseDeck}
        onRandomizeSeats={room.randomizeSeats}
        shuffleVersion={room.shuffleVersion}
        turns={room.turns}
        timer={room.timer}
        autoRandomize={room.autoRandomize}
        onAutoRandomize={room.setAutoRandomize}
        onPassTurn={room.passTurn}
        onAdjustTurn={room.adjustTurn}
        onRoll={room.rollDice}
        onSetEliminated={room.setEliminated}
        onChangeTimer={(action) => {
          void room.changeTimer(action)
        }}
        reveal={{ target: room.revealTo, busy: room.revealBusy, onChange: room.changeReveal }}
        onEndGame={() => {
          void room.changeTimer("pause").then((state) => {
            if (!state) return
            setResultTimer(state)
            setFinishOpen(true)
          })
        }}
      />

      <Dialog open={revealOpen} onOpenChange={setRevealOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Reveal hand</DialogTitle>
            <DialogClose onClose={() => setRevealOpen(false)} />
          </DialogHeader>
          <p className="mb-3 text-sm text-base-content/65">
            Wait for confirmation before showing your hand. Only the chosen player receives your
            video.
          </p>
          <RevealControl
            participants={seated}
            peerId={room.peerId}
            target={room.revealTo}
            busy={room.revealBusy}
            onChange={room.changeReveal}
          />
        </DialogContent>
      </Dialog>
      <HotkeyHelp open={helpOpen} onOpenChange={setHelpOpen} />

      {finishOpen && resultTimer && (
        <FinishGame
          participants={room.participants}
          playedAt={
            resultTimer.started_at === null ? playedAt.current : new Date(resultTimer.started_at)
          }
          timer={resultTimer}
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
