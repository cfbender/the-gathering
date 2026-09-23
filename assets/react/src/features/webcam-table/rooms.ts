import { queryOptions } from "@tanstack/react-query"
import { api } from "@/lib/api"

/** One live webcam table from `GET /api/webcam-table/rooms`. */
export interface ActiveTable {
  id: string
  /** Oldest seat's join time (ms since epoch). */
  started_at: number
  full: boolean
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

/** "Theo, Mara & Cody" for a Join button. */
export function tablePlayerNames(table: ActiveTable) {
  const names = table.players.map((player) => player.name)
  if (names.length <= 1) return names.join("")
  return `${names.slice(0, -1).join(", ")} & ${names.at(-1)}`
}
