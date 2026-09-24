import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useCallback, useEffect, useState, type CSSProperties } from "react"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import { getPlayers } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { turnId } from "./game-modes"
import { MAX_PLAYERS } from "./rooms"
import { SidePanel, type PanelTab } from "./side-panel"
import { TableCameraRail } from "./table-camera-rail"
import { TableDialogs, type TableDialog } from "./table-dialogs"
import { RailResizeHandle, useTablePreferences } from "./table-preferences"
import { TableSettings } from "./table-settings"
import { TableStage } from "./table-stage"
import { decksFor, useTableView } from "./table-view"
import { useCardIdentificationFlow } from "./use-card-identification-flow"
import { useCorrectionUpload } from "./use-correction-upload"
import { useRoomHotkeys } from "./use-room-hotkeys"
import { useTurnSound } from "./use-turn-sound"
import { useVideoStats } from "./video-stats"
import { useWebcamRoom } from "./use-webcam-room"

interface Props {
  roomId: string
}

interface LiveRoomProps extends Props {
  playerId: number
  playerName: string
  decks: DeckSummary[]
}

function useInviteLink() {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = window.setTimeout(() => setCopied(false), 2500)
    return () => window.clearTimeout(timer)
  }, [copied])
  return {
    copied,
    copy: () => {
      void navigator.clipboard.writeText(window.location.href)
      setCopied(true)
    },
  }
}

/** The table layout: camera rail, active board, and side panel, with its dialogs. */
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
  const [dialog, setDialog] = useState<TableDialog>(null)
  const [panelOpen, setPanelOpen] = useState(true)
  const [panelTab, setPanelTab] = useState<PanelTab>("table")
  const invite = useInviteLink()
  const toggleCamera = () => {
    preferences.update({ cameraEnabled: room.cameraOff })
    room.toggleCamera()
  }
  const openHelp = useCallback(() => setDialog({ kind: "help" }), [])
  const view = useTableView({
    room,
    preferences,
    playerId,
    playerName,
    decks,
    toggleCamera,
    openReveal: () => setDialog({ kind: "reveal" }),
  })
  const videoStats = useVideoStats(preferences.stats, room.getPeerStats)
  useTurnSound(
    preferences.turnSound,
    room.turns.active_player_id,
    turnId(room.participants, playerId, room.mode) ?? playerId,
  )
  const corrections = useCorrectionUpload()
  const flow = useCardIdentificationFlow({
    room,
    seated: view.seated,
    decks,
    playerName,
    corrections,
    blocked: dialog?.kind === "help" || dialog?.kind === "finish",
  })
  useRoomHotkeys(view, flow, {
    togglePanel: () => setPanelOpen((open) => !open),
    showTab: (tab) => {
      setPanelTab(tab)
      setPanelOpen(true)
    },
    openHelp,
  })

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
      <TableCameraRail view={view} videoStats={videoStats} />

      <RailResizeHandle
        rail="camera"
        reversed={preferences.panelLeft}
        width={preferences.camera}
        onChange={(width) => preferences.setWidth("camera", width)}
      />

      <TableStage view={view} flow={flow} />

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
        mode={room.mode}
        onModeChange={room.setMode}
        onMoveSeat={room.moveSeat}
        spectating={room.spectating}
        isOwner={room.isOwner}
        left={preferences.panelLeft}
        onHelp={openHelp}
        settings={
          <TableSettings
            preferences={preferences}
            room={{ ...room, toggleCamera }}
            corrections={corrections}
            recognizer={flow.recognizer.state}
            onHelp={openHelp}
          />
        }
        open={panelOpen}
        tab={panelTab}
        onOpenChange={setPanelOpen}
        onTabChange={setPanelTab}
        participants={view.seated}
        localParticipant={view.localParticipant}
        maxPlayers={MAX_PLAYERS}
        playerDecks={decksFor(view, view.localParticipant)}
        decks={decks}
        events={room.events}
        status={room.status}
        error={room.error}
        connectedPeers={Object.keys(room.streams).length}
        connectionStates={room.connectionStates}
        iceServers={room.iceServers}
        recognizer={flow.recognizer.state}
        identifiedCards={room.identifiedCards}
        gallerySearchable
        onSearch={flow.recognizer.search}
        onPreviewCard={flow.previewEntry}
        onPreviewArt={flow.previewArt}
        onRemoveCard={room.removeCard}
        onClearOwnCards={room.clearOwnCards}
        inviteCopied={invite.copied}
        onInvite={invite.copy}
        onChooseDeck={room.chooseDeck}
        onStartGame={room.startGame}
        shuffleVersion={room.shuffleVersion}
        turns={room.turns}
        timer={room.timer}
        onPassTurn={room.passTurn}
        onAdjustTurn={room.adjustTurn}
        onRoll={room.rollDice}
        onChangeTimer={(action) => {
          void room.changeTimer(action)
        }}
        reveal={{ target: room.revealTo, busy: room.revealBusy, onChange: room.changeReveal }}
        onEndGame={() => {
          void room.changeTimer("pause").then((timer) => {
            if (timer) setDialog({ kind: "finish", timer })
          })
        }}
      />

      <TableDialogs view={view} dialog={dialog} onDialogChange={setDialog} />
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
