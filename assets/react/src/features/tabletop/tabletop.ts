import type { DeckSummary } from "@/features/decks/decks"
import {
  TABLE_MODES,
  formatLabel,
  isDuelMode,
  isGameFormat,
  type GameFormat,
  type TableMode,
} from "@/features/games/game-format"
import type { PlayerSummary } from "@/features/games/games"

/** Tabletop is one device shared by everyone at a physical table: life totals, commander
 * damage, poison, and turns live in this browser. Finished games are recorded in game history
 * only when every seat is a known player in a recorded format. */

/** Two-Headed Giant's shared team life is not modelled here. */
export const TABLETOP_FORMATS = TABLE_MODES.filter((mode) => mode.value !== "two_headed_giant")
export type TabletopFormat = Exclude<TableMode, "two_headed_giant">

export const MIN_SEATS = 2
export const MAX_SEATS = 6

export function startingLife(format: TabletopFormat) {
  if (isDuelMode(format)) return 20
  return format === "pauper_commander" ? 30 : 40
}

/** Seat counts a format allows: duels are 2 players and Five Star is exactly 5. */
export function seatCounts(format: TabletopFormat): number[] {
  if (isDuelMode(format)) return [2]
  if (format === "five_star") return [5]
  return [2, 3, 4, 5, 6]
}

/** Combat damage from one commander that knocks a player out. */
export function lethalCommanderDamage(format: TabletopFormat) {
  return format === "pauper_commander" ? 16 : 21
}

/** Formats with commanders track commander damage. */
export function hasCommanders(format: TabletopFormat) {
  return format !== "standard" && format !== "pauper"
}

// --- Layouts --------------------------------------------------------------------------------

/** Degrees a seat's panel turns so its bottom edge faces the player sitting there. */
export type Rotation = 0 | 90 | 180 | 270

export interface SeatArea {
  row: number
  col: number
  rowSpan?: number
  colSpan?: number
  rotation: Rotation
}

export interface Layout {
  id: string
  label: string
  rows: number
  cols: number
  /** Seats clockwise around the table, as seen from above with the device upright. */
  seats: SeatArea[]
  /** Seats for a landscape screen, when turning the portrait seats a quarter would not do. */
  landscape?: Pick<Layout, "rows" | "cols" | "seats">
}

const bottom = (row: number, colSpan = 2): SeatArea => ({ row, col: 1, colSpan, rotation: 0 })
const top = (colSpan = 2): SeatArea => ({ row: 1, col: 1, colSpan, rotation: 180 })
const left = (row: number): SeatArea => ({ row, col: 1, rotation: 90 })
const right = (row: number): SeatArea => ({ row, col: 2, rotation: 270 })

export const LAYOUTS: Record<number, Layout[]> = {
  2: [
    { id: "face-off", label: "Face to face", rows: 2, cols: 1, seats: [bottom(2, 1), top(1)] },
    { id: "side-by-side", label: "Side by side", rows: 1, cols: 2, seats: [left(1), right(1)] },
    {
      // Both players read the device from its left edge; a landscape screen is already upright
      // for them, so its seats stand side by side without turning.
      id: "same-side",
      label: "Same side",
      rows: 2,
      cols: 1,
      seats: [left(1), left(2)],
      landscape: {
        rows: 1,
        cols: 2,
        seats: [
          { row: 1, col: 1, rotation: 0 },
          { row: 1, col: 2, rotation: 0 },
        ],
      },
    },
  ],
  3: [
    { id: "head", label: "One at the head", rows: 2, cols: 2, seats: [left(2), top(), right(2)] },
    {
      id: "foot",
      label: "One at the foot",
      rows: 2,
      cols: 2,
      seats: [bottom(2), left(1), right(1)],
    },
  ],
  4: [
    {
      id: "sides",
      label: "Two a side",
      rows: 2,
      cols: 2,
      seats: [left(2), left(1), right(1), right(2)],
    },
    {
      id: "ends",
      label: "One at each end",
      rows: 3,
      cols: 2,
      seats: [bottom(3), left(2), top(), right(2)],
    },
  ],
  5: [
    {
      id: "head",
      label: "One at the head",
      rows: 3,
      cols: 2,
      seats: [left(3), left(2), top(), right(2), right(3)],
    },
    {
      id: "foot",
      label: "One at the foot",
      rows: 3,
      cols: 2,
      seats: [bottom(3), left(2), left(1), right(1), right(2)],
    },
  ],
  6: [
    {
      id: "sides",
      label: "Three a side",
      rows: 3,
      cols: 2,
      seats: [left(3), left(2), left(1), right(1), right(2), right(3)],
    },
    {
      id: "ends",
      label: "One at each end",
      rows: 4,
      cols: 2,
      seats: [bottom(4), left(3), left(2), top(), right(2), right(3)],
    },
  ],
}

export function layoutsFor(count: number): Layout[] {
  return LAYOUTS[count] ?? LAYOUTS[4]!
}

export function findLayout(count: number, id: string): Layout {
  const layouts = layoutsFor(count)
  return layouts.find((layout) => layout.id === id) ?? layouts[0]!
}

/** The layout for a landscape screen: the portrait layout turned a quarter clockwise, so the
 * same seats keep facing the same table edges once the device lies sideways. */
export function landscape(layout: Layout): Layout {
  if (layout.landscape) return { ...layout, ...layout.landscape }
  return {
    ...layout,
    rows: layout.cols,
    cols: layout.rows,
    seats: layout.seats.map((seat) => {
      const rowSpan = seat.rowSpan ?? 1
      return {
        row: seat.col,
        col: layout.rows - (seat.row + rowSpan - 1) + 1,
        rowSpan: seat.colSpan ?? 1,
        colSpan: rowSpan,
        rotation: ((seat.rotation + 90) % 360) as Rotation,
      }
    }),
  }
}

// --- Game state -----------------------------------------------------------------------------

export type SeatOccupant =
  | { kind: "player"; id: number; name: string; avatarUrl: string | null }
  | { kind: "guest"; name: string }

export interface SeatDeck {
  id: number
  name: string
  commanderName: string
  partnerName: string | null
  artUrl: string | null
  partnerArtUrl: string | null
}

export interface TabletopSeat {
  /** `null` until someone sits down; an empty seat plays as a guest. */
  occupant: SeatOccupant | null
  deck: SeatDeck | null
  life: number
  poison: number
  /** Commander damage taken, keyed by `commanderKey`. */
  commanderDamage: Record<string, number>
  conceded: boolean
}

export interface TabletopGame {
  format: TabletopFormat
  startingLife: number
  layoutId: string
  phase: "seating" | "playing"
  seats: TabletopSeat[]
  startedAt: string | null
  /** The seat that took the first turn; `null` while turns are not tracked. */
  firstSeat: number | null
  activeSeat: number | null
  /** The round the active seat is on, from 1. */
  turn: number
}

export interface TabletopSettings {
  format: TabletopFormat
  seatCount: number
  layoutId: string
  startingLife: number
}

export const DEFAULT_SETTINGS: TabletopSettings = {
  format: "commander",
  seatCount: 4,
  layoutId: "sides",
  startingLife: 40,
}

function emptySeat(life: number, occupant: SeatOccupant | null = null): TabletopSeat {
  return { occupant, deck: null, life, poison: 0, commanderDamage: {}, conceded: false }
}

export function newGame(settings: TabletopSettings, previous?: TabletopGame): TabletopGame {
  return {
    format: settings.format,
    startingLife: settings.startingLife,
    layoutId: settings.layoutId,
    phase: "seating",
    // Players who stay for another game keep their seats (and decks) in the new layout.
    seats: Array.from({ length: settings.seatCount }, (_, index) => {
      const kept = previous?.seats[index]
      return { ...emptySeat(settings.startingLife, kept?.occupant), deck: kept?.deck ?? null }
    }),
    startedAt: null,
    firstSeat: null,
    activeSeat: null,
    turn: 0,
  }
}

/** Starts play (or a rematch): every total resets and the clock starts. */
export function startGame(game: TabletopGame, now = new Date()): TabletopGame {
  return {
    ...game,
    phase: "playing",
    seats: game.seats.map((seat) => ({
      ...emptySeat(game.startingLife, seat.occupant),
      deck: seat.deck,
    })),
    startedAt: now.toISOString(),
    firstSeat: null,
    activeSeat: null,
    turn: 0,
  }
}

export function seatName(seat: TabletopSeat, index: number) {
  return seat.occupant?.name ?? `Player ${index + 1}`
}

export function deckFromSummary(deck: DeckSummary): SeatDeck {
  return {
    id: deck.id,
    name: deck.name,
    commanderName: deck.commander_name,
    partnerName: deck.partner_name,
    artUrl: deck.commander_art_crop_url,
    partnerArtUrl: deck.partner_art_crop_url,
  }
}

export function playerOccupant(player: PlayerSummary): SeatOccupant {
  return { kind: "player", id: player.id, name: player.name, avatarUrl: player.avatar_url }
}

export function sit(
  game: TabletopGame,
  index: number,
  occupant: SeatOccupant | null,
  deck: SeatDeck | null = null,
): TabletopGame {
  return updateSeat(game, index, (seat) => ({ ...seat, occupant, deck }))
}

function updateSeat(
  game: TabletopGame,
  index: number,
  change: (seat: TabletopSeat) => TabletopSeat,
): TabletopGame {
  return {
    ...game,
    seats: game.seats.map((seat, at) => (at === index ? change(seat) : seat)),
  }
}

const LIFE_LIMIT = 999

export function changeLife(game: TabletopGame, index: number, delta: number): TabletopGame {
  return updateSeat(game, index, (seat) => ({
    ...seat,
    life: Math.max(-LIFE_LIMIT, Math.min(LIFE_LIMIT, seat.life + delta)),
  }))
}

export function changePoison(game: TabletopGame, index: number, delta: number): TabletopGame {
  return updateSeat(game, index, (seat) => ({
    ...seat,
    poison: Math.max(0, Math.min(99, seat.poison + delta)),
  }))
}

/** Identifies one commander at a seat: partners deal commander damage separately. */
export function commanderKey(sourceSeat: number, partner = false) {
  return partner ? `${sourceSeat}p` : String(sourceSeat)
}

/** Commander damage is combat damage, so it also comes off the player's life. */
export function changeCommanderDamage(
  game: TabletopGame,
  index: number,
  key: string,
  delta: number,
): TabletopGame {
  const seat = game.seats[index]
  if (!seat) return game
  const before = seat.commanderDamage[key] ?? 0
  const after = Math.max(0, Math.min(99, before + delta))
  if (after === before) return game
  return changeLife(
    updateSeat(game, index, (current) => ({
      ...current,
      commanderDamage: { ...current.commanderDamage, [key]: after },
    })),
    index,
    before - after,
  )
}

export function setConceded(game: TabletopGame, index: number, conceded: boolean): TabletopGame {
  return updateSeat(game, index, (seat) => ({ ...seat, conceded }))
}

/** Why a seat is out of the game, or `null` while it is still in. */
export function eliminationReason(
  seat: TabletopSeat,
  format: TabletopFormat,
): "conceded" | "life" | "poison" | "commander" | null {
  if (seat.conceded) return "conceded"
  if (seat.life <= 0) return "life"
  if (seat.poison >= 10) return "poison"
  const lethal = lethalCommanderDamage(format)
  if (Object.values(seat.commanderDamage).some((damage) => damage >= lethal)) return "commander"
  return null
}

export function remainingSeats(game: TabletopGame): number[] {
  return game.seats.flatMap((seat, index) =>
    eliminationReason(seat, game.format) === null ? [index] : [],
  )
}

/** The last seat standing, once everyone else is out. */
export function lastStanding(game: TabletopGame): number | null {
  if (game.phase !== "playing" || game.seats.length < MIN_SEATS) return null
  const remaining = remainingSeats(game)
  return remaining.length === 1 ? remaining[0]! : null
}

// --- Turns ----------------------------------------------------------------------------------

export function chooseFirstSeat(game: TabletopGame, index: number): TabletopGame {
  return { ...game, firstSeat: index, activeSeat: index, turn: 1 }
}

/** Passes the turn clockwise to the next seat still in the game. A new round starts whenever
 * play passes the first player's seat. */
export function passTurn(game: TabletopGame): TabletopGame {
  const { activeSeat, firstSeat } = game
  if (activeSeat === null || firstSeat === null) return game
  const count = game.seats.length
  let turn = game.turn
  for (let step = 1; step <= count; step += 1) {
    const next = (activeSeat + step) % count
    // Distance from the first seat, clockwise: wrapping past it begins the next round.
    if ((next - firstSeat + count) % count <= (activeSeat - firstSeat + count) % count) {
      turn = game.turn + 1
    }
    if (eliminationReason(game.seats[next]!, game.format) === null) {
      return { ...game, activeSeat: next, turn }
    }
  }
  return game
}

// --- Recording ------------------------------------------------------------------------------

export type RecordBlocker =
  | { kind: "format"; format: TabletopFormat }
  /** Named guests, plus whether any seat is still empty (empty seats play as guests). */
  | { kind: "guests"; names: string[]; empty: boolean }

/** Why this game cannot go into game history, or `null` when it can. */
export function recordBlocker(game: TabletopGame): RecordBlocker | null {
  if (!isGameFormat(game.format)) return { kind: "format", format: game.format }
  const names = game.seats.flatMap((seat) =>
    seat.occupant?.kind === "guest" ? [seat.occupant.name] : [],
  )
  const empty = game.seats.some((seat) => seat.occupant === null)
  return names.length > 0 || empty ? { kind: "guests", names, empty } : null
}

export function recordBlockerMessage(blocker: RecordBlocker) {
  if (blocker.kind === "format") {
    return `${formatLabel(blocker.format)} games aren't recorded in game history.`
  }
  if (blocker.empty) return "Empty seats play as guests. Games with guests aren't recorded."
  const [first, ...others] = blocker.names
  return others.length === 0
    ? `${first} is a guest, so this game won't be recorded.`
    : `${blocker.names.length} guests are playing, so this game won't be recorded.`
}

/** Seats in turn order, starting with the first player when one was chosen. */
export function turnOrder(game: TabletopGame): number[] {
  const start = game.firstSeat ?? 0
  return game.seats.map((_, step) => (start + step) % game.seats.length)
}

export interface RecordDetails {
  /** A seat index, or `"draw"`. */
  winner: number | "draw"
  durationMinutes: string
  turns: string
  winCondition: string
}

/** The `POST /api/games` body for a game every seat of which is a known player. */
export function gamePayload(game: TabletopGame, details: RecordDetails) {
  return {
    format: game.format as GameFormat,
    played_at: game.startedAt ?? new Date().toISOString(),
    duration_minutes: details.durationMinutes ? Number(details.durationMinutes) : null,
    turns: details.turns ? Number(details.turns) : null,
    win_condition: details.winCondition || null,
    seats: turnOrder(game).map((index, order) => {
      const seat = game.seats[index]!
      return {
        player_id: seat.occupant?.kind === "player" ? seat.occupant.id : null,
        deck_id: seat.deck?.id ?? null,
        seat: order + 1,
        result:
          details.winner === "draw"
            ? ("draw" as const)
            : details.winner === index
              ? ("win" as const)
              : ("loss" as const),
      }
    }),
  }
}

/** Whole minutes since the game started, at least one. */
export function elapsedMinutes(game: TabletopGame, now = Date.now()) {
  if (!game.startedAt) return null
  return Math.max(1, Math.round((now - Date.parse(game.startedAt)) / 60_000))
}
