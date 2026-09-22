import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useEffect, useRef, useState } from "react"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import { getPlayers } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"
import { ActiveBoard, CameraTile, OpenSeat, capturePoint } from "./board"
import { CardSuggestions } from "./card-suggestions"
import { FinishGame } from "./finish-game"
import { SeatBar, TileCommanderRow } from "./seat-bar"
import { SidePanel, type PanelTab } from "./side-panel"
import { useWebcamRoom, type TableParticipant } from "./use-webcam-room"

const MAX_PLAYERS = 4

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

function LiveRoom({ roomId, playerId, playerName, decks }: LiveRoomProps) {
  const room = useWebcamRoom(roomId, playerId, null)
  const [inviteCopied, setInviteCopied] = useState(false)
  const [finishOpen, setFinishOpen] = useState(false)
  const [panelOpen, setPanelOpen] = useState(true)
  const [panelTab, setPanelTab] = useState<PanelTab>("table")
  const playedAt = useRef(new Date())
  const board = useActiveBoard(room.participants, room.peerId)

  const localParticipant: TableParticipant = room.participants.find(
    (participant) => participant.peer_id === room.peerId,
  ) ?? {
    peer_id: room.peerId,
    player_id: playerId,
    player_name: playerName,
    life: room.life,
    muted: true,
    camera_off: room.cameraOff,
    joined_at: Number.MAX_SAFE_INTEGER,
  }
  const seated = room.participants.some((participant) => participant.peer_id === room.peerId)
    ? room.participants
    : [localParticipant, ...room.participants]
  const activeParticipant =
    seated.find((participant) => participant.peer_id === board.selectedPeerId) ?? localParticipant
  const decksFor = (participant: TableParticipant) =>
    decks.filter((deck) => deck.player_id === participant.player_id)
  const chooseFor = (participant: TableParticipant) => (deckId: number) =>
    room.suggestDeck(participant.peer_id, deckId)
  const captureOwner = room.capture
    ? seated.find((participant) => participant.peer_id === room.capture?.peerId)
    : undefined
  const suggestions = captureOwner ? decksFor(captureOwner).slice(0, 5) : []

  useEffect(() => {
    function choose(event: KeyboardEvent) {
      if (!room.capture || event.key < "1" || event.key > "5") return
      if (event.target instanceof HTMLElement && event.target.matches("input, textarea, select"))
        return
      const deck = suggestions[Number(event.key) - 1]
      if (deck) room.suggestDeck(room.capture.peerId, deck.id)
    }
    window.addEventListener("keydown", choose)
    return () => window.removeEventListener("keydown", choose)
  }, [room, suggestions])

  useEffect(() => {
    if (!inviteCopied) return
    const timer = window.setTimeout(() => setInviteCopied(false), 2500)
    return () => window.clearTimeout(timer)
  }, [inviteCopied])

  const seatBarFor = (participant: TableParticipant, size: "board" | "tile") => (
    <SeatBar
      participant={participant}
      local={participant.peer_id === room.peerId}
      decks={decksFor(participant)}
      size={size}
      onChooseDeck={chooseFor(participant)}
      onChangeLife={room.changeLife}
      onToggleCamera={room.toggleCamera}
    />
  )

  return (
    <div className="grid h-dvh grid-rows-[auto_minmax(0,1fr)_auto] bg-black text-white lg:grid-cols-[13rem_minmax(0,1fr)_auto] lg:grid-rows-1">
      <aside
        className="flex gap-1.5 overflow-x-auto p-1.5 lg:flex-col lg:overflow-x-hidden lg:overflow-y-auto"
        aria-label="Player cameras"
      >
        {seated.map((participant) => (
          <div
            key={participant.peer_id}
            className="w-44 shrink-0 overflow-hidden rounded-sm lg:w-auto"
          >
            <CameraTile
              participant={participant}
              local={participant.peer_id === room.peerId}
              active={participant.peer_id === activeParticipant.peer_id}
              stream={streamFor(participant, room.peerId, room.localStream, room.streams)}
              onActivate={() => board.select(participant.peer_id)}
            />
            {seatBarFor(participant, "tile")}
            <TileCommanderRow
              participant={participant}
              decks={decksFor(participant)}
              onChooseDeck={chooseFor(participant)}
            />
          </div>
        ))}
        {Array.from({ length: Math.max(0, MAX_PLAYERS - seated.length) }, (_, index) => (
          <div key={`open-${index}`} className="hidden w-44 shrink-0 lg:block lg:w-auto">
            <OpenSeat />
          </div>
        ))}
      </aside>

      <section className="relative flex min-h-0 min-w-0 flex-col" aria-label="Active board">
        <div className="relative min-h-0 flex-1">
          <ActiveBoard
            participant={activeParticipant}
            local={activeParticipant.peer_id === room.peerId}
            stream={streamFor(activeParticipant, room.peerId, room.localStream, room.streams)}
            pinned={board.pinned}
            onTogglePin={board.togglePin}
            onInspect={(event) => {
              const point = capturePoint(event)
              if (point) room.requestCapture(activeParticipant.peer_id, point.x, point.y)
            }}
          />
          {room.capture && captureOwner && (
            <CardSuggestions
              capture={room.capture}
              playerName={captureOwner.player_name}
              suggestions={suggestions}
              onChoose={(deckId) => room.suggestDeck(captureOwner.peer_id, deckId)}
              onDismiss={room.dismissCapture}
            />
          )}
        </div>
        {seatBarFor(activeParticipant, "board")}
      </section>

      <SidePanel
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
        inviteCopied={inviteCopied}
        onInvite={() => {
          void navigator.clipboard.writeText(window.location.href)
          setInviteCopied(true)
        }}
        onChooseDeck={room.chooseDeck}
        onRandomizeSeats={room.randomizeSeats}
        onEndGame={() => setFinishOpen(true)}
      />

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
