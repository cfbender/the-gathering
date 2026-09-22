import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Link, useNavigate } from "@tanstack/react-router"
import { Check, Copy, DoorOpen, Radio, Users, Video } from "lucide-react"
import { useEffect, useRef, useState, type MouseEvent } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { getDecks, type DeckSummary } from "@/features/decks/decks"
import {
  getPlayers,
  invalidateGameRelated,
  WIN_CONDITIONS,
  type Game,
} from "@/features/games/games"
import { api, ApiError } from "@/lib/api"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { buildGamePayload } from "./game-result"
import { useWebcamRoom, type CapturedCard, type TableParticipant } from "./use-webcam-room"

interface Props {
  roomId: string
}

function StreamVideo({
  stream,
  muted = false,
  className,
}: {
  stream: MediaStream
  muted?: boolean
  className?: string
}) {
  return (
    <video
      ref={(video) => {
        if (video && video.srcObject !== stream) video.srcObject = stream
      }}
      className={cn("h-full w-full", className)}
      autoPlay
      playsInline
      muted={muted}
    />
  )
}

function EmptyVideo({ label = "Connecting…" }: { label?: string }) {
  return (
    <div className="text-neutral-content/45 grid h-full w-full place-items-center bg-black/30">
      <div className="text-center text-sm">
        <Video className="mx-auto mb-2 size-7" />
        {label}
      </div>
    </div>
  )
}

function ActiveBoard({
  participant,
  stream,
  local,
  onInspect,
}: {
  participant: TableParticipant
  stream?: MediaStream
  local: boolean
  onInspect: (event: MouseEvent<HTMLButtonElement>) => void
}) {
  return (
    <button
      type="button"
      className="group relative flex h-full w-full items-center justify-center overflow-hidden bg-black text-left"
      onClick={onInspect}
      aria-label={`Inspect ${participant.player_name}'s board`}
    >
      {stream ? (
        <StreamVideo stream={stream} muted={local} className="object-contain" />
      ) : (
        <EmptyVideo />
      )}
      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex items-end justify-between bg-gradient-to-t from-black/90 to-transparent p-5 pt-16 text-white">
        <div>
          <div className="text-lg font-bold">{participant.player_name}</div>
          <div className="text-sm text-white/70">{participant.deck_name ?? "No deck selected"}</div>
        </div>
        {local && <span className="badge badge-sm">You</span>}
      </div>
      <span className="pointer-events-none absolute top-4 right-4 rounded-full bg-black/70 px-3 py-1.5 text-xs text-white opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
        Click a card
      </span>
    </button>
  )
}

function CameraThumbnail({
  participant,
  stream,
  local,
  active,
  onActivate,
}: {
  participant: TableParticipant
  stream?: MediaStream
  local: boolean
  active: boolean
  onActivate: () => void
}) {
  return (
    <button
      type="button"
      className={cn(
        "relative aspect-video overflow-hidden rounded-lg border-2 bg-black text-left transition",
        active
          ? "border-primary ring-primary/30 ring-2"
          : "border-base-300 hover:border-primary/60",
      )}
      onClick={onActivate}
      aria-pressed={active}
      aria-label={`Show ${participant.player_name}'s board`}
    >
      {stream ? (
        <StreamVideo stream={stream} muted={local} className="object-cover" />
      ) : (
        <EmptyVideo />
      )}
      <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/90 to-transparent px-2.5 pt-7 pb-2 text-white">
        <div className="truncate text-sm font-bold">
          {participant.player_name} {local ? "(you)" : ""}
        </div>
        <div className="truncate text-[0.68rem] text-white/65">
          {participant.deck_name ?? "Choose a deck"}
        </div>
      </div>
    </button>
  )
}

function FinishGame({
  participants,
  playedAt,
  onOpenChange,
}: {
  participants: TableParticipant[]
  playedAt: Date
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [winner, setWinner] = useState("")
  const [duration] = useState(() =>
    Math.max(1, Math.round((Date.now() - playedAt.getTime()) / 60_000)).toString(),
  )
  const [turns, setTurns] = useState("")
  const [winCondition, setWinCondition] = useState("")
  const [notes, setNotes] = useState("")
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: Game }>("/api/games", {
        method: "POST",
        body: JSON.stringify(
          buildGamePayload(participants, {
            playedAt,
            winner,
            duration,
            turns,
            winCondition,
            notes,
          }),
        ),
      }).then((body) => body.data),
    onSuccess: async (game) => {
      await invalidateGameRelated(queryClient)
      void navigate({ to: "/games/$gameId", params: { gameId: String(game.id) } })
    },
  })
  const error = mutation.error instanceof ApiError ? mutation.error.detail : null

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <div>
            <DialogTitle>Record game result</DialogTitle>
            <p className="text-base-content/60 mt-1 text-sm">
              Confirm the outcome before adding this game to history.
            </p>
          </div>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <form
          className="grid gap-5 p-5 sm:p-6 md:grid-cols-2"
          onSubmit={(event) => {
            event.preventDefault()
            mutation.mutate()
          }}
        >
          <fieldset className="md:col-span-2">
            <legend className="mb-2 text-sm font-bold">Winner</legend>
            <ToggleGroup
              type="single"
              value={winner}
              onValueChange={setWinner}
              aria-label="Winner"
              className="grid gap-2 sm:grid-cols-2"
            >
              {participants.map((participant) => (
                <ToggleGroupItem
                  key={participant.peer_id}
                  value={participant.peer_id}
                  className={cn(
                    "btn h-auto min-h-12 justify-start px-4 py-3",
                    winner === participant.peer_id ? "btn-primary" : "btn-outline",
                  )}
                >
                  {participant.player_name}
                  <span className="ml-auto text-xs opacity-65">
                    {participant.deck_name ?? "No deck"}
                  </span>
                </ToggleGroupItem>
              ))}
              <ToggleGroupItem
                value="draw"
                className={cn(
                  "btn h-auto min-h-12 px-4 py-3",
                  winner === "draw" ? "btn-primary" : "btn-outline",
                )}
              >
                Draw
              </ToggleGroupItem>
            </ToggleGroup>
          </fieldset>
          <label className="form-control">
            <span className="label-text mb-1">Duration</span>
            <div className="input input-bordered bg-base-200 flex items-center font-semibold">
              {duration} {duration === "1" ? "minute" : "minutes"}
            </div>
            <span className="text-base-content/50 mt-1 text-xs">Tracked from room creation</span>
          </label>
          <label className="form-control">
            <span className="label-text mb-1">Turns</span>
            <input
              className="input input-bordered"
              type="number"
              min="1"
              placeholder="Optional"
              value={turns}
              onChange={(event) => setTurns(event.target.value)}
            />
          </label>
          <label className="form-control md:col-span-2">
            <span className="label-text mb-1">Win condition</span>
            <select
              className="select select-bordered w-full"
              value={winCondition}
              onChange={(event) => setWinCondition(event.target.value)}
            >
              <option value="">Not recorded</option>
              {WIN_CONDITIONS.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="form-control md:col-span-2">
            <span className="label-text mb-1">Notes</span>
            <textarea
              className="textarea textarea-bordered"
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
            />
          </label>
          {error && <div className="alert alert-error md:col-span-2">{error}</div>}
          {participants.length < 2 && (
            <p className="text-base-content/60 text-sm md:col-span-2">
              At least two players must be in the room to record a game.
            </p>
          )}
          <div className="flex justify-end gap-2 md:col-span-2">
            <button type="button" className="btn btn-ghost" onClick={() => onOpenChange(false)}>
              Back to game
            </button>
            <button
              className="btn btn-primary min-w-36"
              disabled={participants.length < 2 || !winner || mutation.isPending}
            >
              {mutation.isPending ? "Recording…" : "Record result"}
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function capturePoint(event: MouseEvent<HTMLButtonElement>) {
  const video = event.currentTarget.querySelector("video")
  if (!video || !video.videoWidth || !video.videoHeight) return null

  const bounds = event.currentTarget.getBoundingClientRect()
  const sourceRatio = video.videoWidth / video.videoHeight
  const boundsRatio = bounds.width / bounds.height
  const renderedWidth = sourceRatio > boundsRatio ? bounds.width : bounds.height * sourceRatio
  const renderedHeight = sourceRatio > boundsRatio ? bounds.width / sourceRatio : bounds.height
  const left = bounds.left + (bounds.width - renderedWidth) / 2
  const top = bounds.top + (bounds.height - renderedHeight) / 2

  return {
    x: Math.max(0, Math.min(1, (event.clientX - left) / renderedWidth)),
    y: Math.max(0, Math.min(1, (event.clientY - top) / renderedHeight)),
  }
}

function CardSuggestions({
  capture,
  suggestions,
  onChoose,
  onDismiss,
}: {
  capture: CapturedCard
  suggestions: DeckSummary[]
  onChoose: (deckId: number) => void
  onDismiss: () => void
}) {
  return (
    <section className="absolute right-4 bottom-4 left-4 z-10 mx-auto max-w-3xl overflow-hidden rounded-2xl border border-white/15 bg-black/85 text-white shadow-2xl backdrop-blur-xl">
      <div className="grid gap-4 p-4 sm:grid-cols-[8rem_1fr]">
        <img
          className="aspect-square w-full rounded-xl object-cover"
          src={capture.image}
          alt="Native camera crop around the clicked card"
        />
        <div className="min-w-0">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h2 className="font-bold">Choose this player’s deck</h2>
              <p className="mt-1 text-xs text-white/55">
                Native {capture.nativeWidth}×{capture.nativeHeight} crop · deck suggestions until ML
                artifacts ship
              </p>
            </div>
            <button type="button" className="btn btn-ghost btn-xs" onClick={onDismiss}>
              Dismiss
            </button>
          </div>
          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            {suggestions.map((deck, index) => (
              <button
                key={deck.id}
                type="button"
                className="btn btn-sm justify-start border-white/15 bg-white/10 text-white hover:bg-white/20"
                onClick={() => onChoose(deck.id)}
              >
                <kbd className="kbd kbd-xs text-black">{index + 1}</kbd>
                <span className="truncate">{deck.commander_name}</span>
              </button>
            ))}
            {suggestions.length === 0 && (
              <p className="text-sm text-white/65">No decks are recorded for this player yet.</p>
            )}
          </div>
        </div>
      </div>
    </section>
  )
}

function streamFor(
  participant: TableParticipant,
  peerId: string,
  localStream: MediaStream | null,
  streams: Record<string, MediaStream>,
) {
  return participant.peer_id === peerId ? (localStream ?? undefined) : streams[participant.peer_id]
}

function LiveRoom({ roomId, playerId, decks }: Props & { playerId: number; decks: DeckSummary[] }) {
  const room = useWebcamRoom(roomId, playerId, null)
  const [copied, setCopied] = useState(false)
  const [finishOpen, setFinishOpen] = useState(false)
  const [activePeerId, setActivePeerId] = useState<string>(room.peerId)
  const playedAt = useRef(new Date())
  const participantByPeer = new Map(
    room.participants.map((participant) => [participant.peer_id, participant]),
  )
  const localParticipant = participantByPeer.get(room.peerId) ?? {
    peer_id: room.peerId,
    player_id: playerId,
    player_name: "You",
  }
  const displayed = [
    localParticipant,
    ...room.participants.filter((item) => item.peer_id !== room.peerId),
  ]
  const activeParticipant =
    displayed.find((participant) => participant.peer_id === activePeerId) ?? localParticipant
  const suggestions = room.capture
    ? decks.filter((deck) => deck.player_id === room.capture?.playerId).slice(0, 5)
    : []
  const playerDecks = decks.filter((deck) => deck.player_id === playerId)

  useEffect(() => {
    function choose(event: KeyboardEvent) {
      if (!room.capture || event.key < "1" || event.key > "5") return
      const deck = suggestions[Number(event.key) - 1]
      if (deck) room.suggestDeck(room.capture.peerId, deck.id)
    }
    window.addEventListener("keydown", choose)
    return () => window.removeEventListener("keydown", choose)
  }, [room, suggestions])

  return (
    <div className="grid min-h-dvh bg-neutral lg:h-dvh lg:grid-cols-[minmax(0,1fr)_22rem] lg:overflow-hidden">
      <section className="relative min-h-[58dvh] overflow-hidden bg-black lg:min-h-0">
        <ActiveBoard
          participant={activeParticipant}
          local={activeParticipant.peer_id === room.peerId}
          stream={streamFor(activeParticipant, room.peerId, room.localStream, room.streams)}
          onInspect={(event) => {
            const point = capturePoint(event)
            if (point) room.requestCapture(activeParticipant.peer_id, point.x, point.y)
          }}
        />
        <div className="pointer-events-none absolute top-4 left-4 rounded-full bg-black/70 px-3 py-1.5 text-xs text-white backdrop-blur">
          <span className="mr-2 inline-block size-2 rounded-full bg-success" />
          {room.error ?? room.status}
        </div>
        {room.capture && (
          <CardSuggestions
            capture={room.capture}
            suggestions={suggestions}
            onChoose={(deckId) => room.suggestDeck(room.capture!.peerId, deckId)}
            onDismiss={room.dismissCapture}
          />
        )}
      </section>

      <aside className="text-base-content bg-base-200 border-base-300 flex min-h-0 flex-col border-l">
        <header className="border-base-300 flex items-center justify-between border-b px-4 py-3">
          <div className="flex items-center gap-2 font-black">
            <Radio className="text-primary size-4" /> Live table
          </div>
          <span className="text-base-content/50 text-xs">{displayed.length}/4 players</span>
        </header>

        <div className="flex-1 space-y-5 overflow-y-auto p-4">
          {room.error && <div className="alert alert-error text-sm">{room.error}</div>}

          <section>
            <h2 className="text-base-content/55 mb-2 text-xs font-bold tracking-wider uppercase">
              Cameras
            </h2>
            <div className="grid grid-cols-2 gap-2">
              {displayed.map((participant) => (
                <CameraThumbnail
                  key={participant.peer_id}
                  participant={participant}
                  local={participant.peer_id === room.peerId}
                  active={participant.peer_id === activeParticipant.peer_id}
                  stream={streamFor(participant, room.peerId, room.localStream, room.streams)}
                  onActivate={() => setActivePeerId(participant.peer_id)}
                />
              ))}
              {Array.from({ length: Math.max(0, 4 - displayed.length) }, (_, index) => (
                <div
                  key={index}
                  className="border-base-300 text-base-content/35 grid aspect-video place-items-center rounded-lg border border-dashed"
                >
                  <Users className="size-5" />
                </div>
              ))}
            </div>
          </section>

          <section>
            <h2 className="text-base-content/55 mb-2 text-xs font-bold tracking-wider uppercase">
              Your commander
            </h2>
            <select
              aria-label="Your deck"
              className="select select-bordered w-full"
              value={localParticipant.deck_id ?? ""}
              onChange={(event) => room.chooseDeck(Number(event.target.value))}
            >
              <option value="" disabled>
                Select your commander
              </option>
              {playerDecks.map((deck) => (
                <option key={deck.id} value={deck.id}>
                  {deck.name} · {deck.commander_name}
                </option>
              ))}
            </select>
          </section>

          <section className="grid gap-2">
            <button
              type="button"
              className="btn btn-outline justify-start"
              onClick={() => {
                void navigator.clipboard.writeText(window.location.href)
                setCopied(true)
              }}
            >
              {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
              {copied ? "Invite link copied" : "Invite players"}
            </button>
            <button
              type="button"
              className="btn btn-error justify-start"
              onClick={() => setFinishOpen(true)}
            >
              <DoorOpen className="size-4" /> End game
            </button>
            <Link to="/games" className="btn btn-ghost justify-start">
              Leave table
            </Link>
          </section>
        </div>
      </aside>

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
      <div className="grid min-h-dvh place-items-center bg-neutral">
        <span className="loading loading-spinner loading-lg" aria-label="Loading table" />
      </div>
    )
  }

  if (!player) {
    return (
      <div className="grid min-h-dvh place-items-center bg-neutral p-6">
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

  return <LiveRoom roomId={roomId} playerId={player.id} decks={decksQuery.data ?? []} />
}
