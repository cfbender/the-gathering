import { queryOptions } from "@tanstack/react-query"
import { api } from "@/lib/api"

/** Seats per table; mirrors `@max_players` in `WebcamTableChannel` and `WebcamTableRooms`. */
export const MAX_PLAYERS = 10

/** One open webcam table from `GET /api/webcam-table/rooms`. Empty tables stay listed until
 * the server closes them after 30 idle minutes. */
export interface ActiveTable {
  id: string
  /** When the room opened (ms since epoch). */
  started_at: number
  full: boolean
  /** Connected seated players; empty when everyone has left. */
  players: { id: number; name: string }[]
}

export function getActiveTables() {
  return api<{ data: ActiveTable[] }>("/api/webcam-table/rooms").then((body) => body.data)
}

/** Live tables, re-checked while the page is open so Join appears without a reload. */
export const activeTablesQuery = queryOptions({
  queryKey: ["webcam-table", "rooms"],
  queryFn: getActiveTables,
  refetchInterval: 15_000,
})

/** "Theo, Mara & Cody" for a Join button, or "Empty table" when no one is seated. */
export function tablePlayerNames(table: ActiveTable) {
  const names = table.players.map((player) => player.name)
  if (names.length === 0) return "Empty table"
  if (names.length === 1) return names.join("")
  return `${names.slice(0, -1).join(", ")} & ${names.at(-1)}`
}
