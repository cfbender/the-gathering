import { api } from "@/lib/api"

export interface CSVImportError {
  line: number
  field: string
  message: string
}

export interface CSVImportSeat {
  line: number
  player: string
  deck: string
  commander: string
  seat: number
  result: "win" | "loss" | "draw"
  mvp_card: string | null
  /** Present for Mythic Track exports only. */
  partner?: string | null
  color_identity?: string
}

export interface ImportWarning {
  line: number
  message: string
}

export interface CSVImportGame {
  game_id: string
  played_at: string
  duration_minutes: number | null
  turns: number | null
  notes: string | null
  seats: CSVImportSeat[]
}

export interface MatchSet<T> {
  create: T[]
  matched: T[]
}

export interface CSVImportPreview {
  valid: boolean
  games: CSVImportGame[]
  players: MatchSet<string | { id: number; name: string }>
  decks: MatchSet<{
    id?: number
    player_id?: number
    player: string
    name: string
    commander: string
  }>
  errors: CSVImportError[]
  warnings: ImportWarning[]
}

export interface CSVImportResult {
  created: number
  skipped: number
  game_ids: number[]
}

export type ImportSource = "csv" | "mythic_track"

/** Where each source's `line` numbers point: CSV rows or positions in the JSON array. */
export const importRowLabel: Record<ImportSource, string> = {
  csv: "Line",
  mythic_track: "Game",
}

export const previewCSV = (csv: string) =>
  api<{ data: CSVImportPreview }>("/api/imports/csv/preview", {
    method: "POST",
    body: JSON.stringify({ csv }),
  }).then((body) => body.data)

export const importCSV = (csv: string) =>
  api<{ data: CSVImportResult }>("/api/imports/csv", {
    method: "POST",
    body: JSON.stringify({ csv }),
  }).then((body) => body.data)

export const previewMythicTrack = (json: string) =>
  api<{ data: CSVImportPreview }>("/api/imports/mythic_track/preview", {
    method: "POST",
    body: JSON.stringify({ json }),
  }).then((body) => body.data)

export const importMythicTrack = (json: string) =>
  api<{ data: CSVImportResult }>("/api/imports/mythic_track", {
    method: "POST",
    body: JSON.stringify({ json }),
  }).then((body) => body.data)

export const previewImport = (source: ImportSource, payload: string) =>
  source === "csv" ? previewCSV(payload) : previewMythicTrack(payload)

export const commitImport = (source: ImportSource, payload: string) =>
  source === "csv" ? importCSV(payload) : importMythicTrack(payload)

/**
 * Snippet users run in the browser console while signed in to mythictrack.com.
 * Mythic Track has no export; this downloads the same game list its client fetches.
 */
export const MYTHIC_TRACK_EXPORT_SNIPPET = `fetch("https://www.api.mythictrack.com/api/games/get", {
  method: "POST",
  credentials: "include",
  headers: { "Content-Type": "application/json" },
  body: "{}",
})
  .then((response) => (response.ok ? response.json() : Promise.reject(response.status)))
  .then((games) => {
    const link = document.createElement("a")
    link.href = URL.createObjectURL(new Blob([JSON.stringify(games)], { type: "application/json" }))
    link.download = "mythic-track-games.json"
    link.click()
    console.log(\`Exported \${games.length} games\`)
  })
  .catch((error) => console.error("Export failed; are you signed in?", error))`
