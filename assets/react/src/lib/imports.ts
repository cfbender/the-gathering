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
}

export interface CSVImportResult {
  created: number
  skipped: number
  game_ids: number[]
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
