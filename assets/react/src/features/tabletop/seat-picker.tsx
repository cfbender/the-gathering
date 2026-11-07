import { useQuery } from "@tanstack/react-query"
import { ChevronLeft, Layers, UserRound, UserRoundPlus, X } from "lucide-react"
import { useEffect, useState, type ReactNode } from "react"
import { CommanderArt } from "@/components/commander-art"
import { PlayerAvatar } from "@/components/player-avatar"
import { commanderNames, getDecks, isRetired } from "@/features/decks/decks"
import { getPlayers, type PlayerSummary } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { seatTint } from "./board-grid"
import {
  deckFromSummary,
  hasCommanders,
  playerOccupant,
  seatName,
  type SeatDeck,
  type SeatOccupant,
  type TabletopFormat,
  type TabletopSeat,
} from "./tabletop"

/** A seat before the game starts: tap it to say who sits there and, for Commander formats,
 * which deck they brought. Everything faces the player at that seat. */
export function SeatPicker({
  seat,
  index,
  format,
  takenPlayerIds,
  onSit,
  onSheetChange,
}: {
  seat: TabletopSeat
  index: number
  format: TabletopFormat
  /** Players already seated elsewhere. */
  takenPlayerIds: number[]
  onSit: (occupant: SeatOccupant | null, deck: SeatDeck | null) => void
  /** Whether this seat's chooser covers the panel, so the board can clear the middle. */
  onSheetChange: (open: boolean) => void
}) {
  const [step, setStep] = useState<"closed" | "player" | "deck">("closed")
  useEffect(() => onSheetChange(step !== "closed"), [step, onSheetChange])
  const occupant = seat.occupant
  const player = occupant?.kind === "player" ? occupant : null

  return (
    <div
      className="rounded-box relative size-full overflow-hidden border border-white/10"
      style={{ background: `color-mix(in oklch, ${seatTint(index)} 35%, black)` }}
    >
      <CommanderArt
        imageUrl={seat.deck?.artUrl}
        partnerImageUrl={seat.deck?.partnerArtUrl}
        imageClassName="opacity-45"
      />
      {step === "closed" ? (
        occupant ? (
          <div className="relative flex size-full flex-col items-center justify-center gap-[4cqh] p-3">
            <button
              type="button"
              className="flex min-w-0 items-center gap-3 rounded-full bg-black/45 py-2 pr-4 pl-2 text-white"
              onClick={() => setStep("player")}
              aria-label={`Change who sits at seat ${index + 1}`}
            >
              {player ? (
                <PlayerAvatar name={player.name} avatarUrl={player.avatarUrl} size="sm" />
              ) : (
                <span className="grid size-9 place-items-center rounded-full bg-white/15">
                  <UserRound className="size-5" />
                </span>
              )}
              <span className="min-w-0 text-left">
                <span className="block truncate font-black">{occupant.name}</span>
                <span className="block text-xs text-white/65">
                  {player ? "Tap to change" : "Guest · not recorded"}
                </span>
              </span>
            </button>
            {player && hasCommanders(format) && (
              <button
                type="button"
                className="btn btn-sm max-w-full border-white/20 bg-black/45 text-white hover:bg-black/60"
                onClick={() => setStep("deck")}
              >
                <Layers className="size-4 shrink-0" />
                <span className="truncate">{seat.deck ? seat.deck.name : "Choose deck"}</span>
              </button>
            )}
          </div>
        ) : (
          <button
            type="button"
            className="relative grid size-full place-items-center p-3"
            onClick={() => setStep("player")}
          >
            <span className="rounded-box flex flex-col items-center gap-2 border-2 border-dashed border-white/35 px-6 py-5 text-white/80">
              <UserRoundPlus className="size-8" />
              <span className="font-bold">Who&rsquo;s sitting here?</span>
              <span className="text-xs text-white/55">{seatName(seat, index)}</span>
            </span>
          </button>
        )
      ) : step === "player" ? (
        <PlayerStep
          current={player?.id ?? null}
          occupied={occupant !== null}
          takenPlayerIds={takenPlayerIds}
          index={index}
          onClose={() => setStep("closed")}
          onChoose={(next) => {
            if (next?.kind === "player" && hasCommanders(format)) {
              onSit(next, next.id === player?.id ? seat.deck : null)
              setStep("deck")
            } else {
              onSit(next, null)
              setStep("closed")
            }
          }}
        />
      ) : (
        player && (
          <DeckStep
            player={player}
            current={seat.deck?.id ?? null}
            onBack={() => setStep("player")}
            onChoose={(deck) => {
              onSit(player, deck)
              setStep("closed")
            }}
          />
        )
      )}
    </div>
  )
}

function Sheet({
  title,
  onBack,
  onClose,
  children,
}: {
  title: string
  onBack?: () => void
  onClose: () => void
  children: ReactNode
}) {
  return (
    <div className="bg-base-100/95 text-base-content absolute inset-0 z-10 flex flex-col backdrop-blur">
      <div className="border-base-300 flex items-center gap-1 border-b px-2 py-1.5">
        {onBack && (
          <button
            type="button"
            className="btn btn-ghost btn-sm btn-square"
            aria-label="Back"
            onClick={onBack}
          >
            <ChevronLeft className="size-4" />
          </button>
        )}
        <h3 className="min-w-0 flex-1 truncate px-1 text-sm font-black">{title}</h3>
        <button
          type="button"
          className="btn btn-ghost btn-sm btn-square"
          aria-label="Close"
          onClick={onClose}
        >
          <X className="size-4" />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-2">{children}</div>
    </div>
  )
}

function PlayerStep({
  current,
  occupied,
  takenPlayerIds,
  index,
  onClose,
  onChoose,
}: {
  current: number | null
  occupied: boolean
  takenPlayerIds: number[]
  index: number
  onClose: () => void
  onChoose: (occupant: SeatOccupant | null) => void
}) {
  const session = useCurrentUser()
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const [guestName, setGuestName] = useState("")
  const viewerId = session.data?.id
  // The signed-in member is most likely sitting down first; everyone else alphabetically.
  const choices = (players.data ?? [])
    .filter((player) => player.archived_at === null && !takenPlayerIds.includes(player.id))
    .sort(
      (a, b) =>
        Number(b.user_id === viewerId) - Number(a.user_id === viewerId) ||
        a.name.localeCompare(b.name),
    )

  return (
    <Sheet title="Who’s sitting here?" onClose={onClose}>
      <form
        className="mb-2 flex gap-1.5"
        onSubmit={(event) => {
          event.preventDefault()
          onChoose({ kind: "guest", name: guestName.trim() || `Guest ${index + 1}` })
        }}
      >
        <input
          className="input input-sm min-w-0 flex-1"
          placeholder={`Guest ${index + 1}`}
          aria-label="Guest name"
          maxLength={24}
          value={guestName}
          onChange={(event) => setGuestName(event.target.value)}
        />
        <button className="btn btn-sm btn-outline shrink-0">Sit as guest</button>
      </form>
      {players.isPending && <div className="skeleton h-24 w-full" />}
      <ul className="grid gap-1">
        {choices.map((player: PlayerSummary) => (
          <li key={player.id}>
            <button
              type="button"
              className={cn(
                "btn btn-ghost h-auto min-h-11 w-full justify-start gap-3 px-2 py-1.5",
                current === player.id && "btn-active",
              )}
              onClick={() => onChoose(playerOccupant(player))}
            >
              <PlayerAvatar name={player.name} avatarUrl={player.avatar_url} size="sm" />
              <span className="truncate">{player.name}</span>
            </button>
          </li>
        ))}
      </ul>
      {occupied && (
        <button
          type="button"
          className="btn btn-ghost btn-sm text-error mt-2 w-full"
          onClick={() => onChoose(null)}
        >
          Leave seat empty
        </button>
      )}
    </Sheet>
  )
}

function DeckStep({
  player,
  current,
  onBack,
  onChoose,
}: {
  player: Extract<SeatOccupant, { kind: "player" }>
  current: number | null
  onBack: () => void
  onChoose: (deck: SeatDeck | null) => void
}) {
  const decks = useQuery({ queryKey: ["decks", {}], queryFn: () => getDecks() })
  const own = (decks.data ?? []).filter((deck) => deck.player_id === player.id && !isRetired(deck))

  return (
    <Sheet title={`${player.name}’s deck`} onBack={onBack} onClose={() => onChoose(null)}>
      {decks.isPending && <div className="skeleton h-24 w-full" />}
      <ul className="grid gap-1.5">
        {own.map((deck) => (
          <li key={deck.id}>
            <button
              type="button"
              className={cn(
                "rounded-field bg-base-200 relative flex h-14 w-full items-center overflow-hidden px-3 text-left text-white",
                current === deck.id && "ring-primary ring-2",
              )}
              onClick={() => onChoose(deckFromSummary(deck))}
            >
              <CommanderArt
                imageUrl={deck.commander_art_crop_url}
                partnerImageUrl={deck.partner_art_crop_url}
              />
              <span className="absolute inset-0 bg-linear-to-r from-black/85 via-black/55 to-black/10" />
              <span className="relative min-w-0">
                <span className="block truncate text-sm font-black">{deck.name}</span>
                <span className="block truncate text-xs text-white/70">{commanderNames(deck)}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      {decks.isSuccess && own.length === 0 && (
        <p className="text-base-content/60 p-2 text-sm">{player.name} has no decks yet.</p>
      )}
      <button
        type="button"
        className="btn btn-ghost btn-sm mt-2 w-full"
        onClick={() => onChoose(null)}
      >
        No deck
      </button>
    </Sheet>
  )
}
