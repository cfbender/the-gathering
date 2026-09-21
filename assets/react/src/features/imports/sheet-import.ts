import { api } from "@/lib/api"

export interface SheetInput {
  text: string
  players: Record<string, number | "new">
  decks: Record<string, number | "new">
  actions: Record<string, number | "create" | "skip">
}

export interface SheetSeat {
  player: string
  deck: string
  player_id: number | string | null
  deck_id: number | "new" | null
  deck_key: string
  kills: number | null
  result: "win" | "loss" | "draw"
}

export interface SheetCandidate {
  id: number
  played_at: string
  notes: string | null
  seats: Array<{
    player_id: number
    player: string
    deck: string | null
    result: string
    kills: number | null
  }>
}

export interface SheetRow {
  key: string
  line: number
  date: string | null
  winner: string
  notes: string
  seats: SheetSeat[]
  kill_counts: Array<{ player: string; kills: number }>
  errors: string[]
  warnings: string[]
  action: number | "create" | "skip"
  status: "changed" | "unchanged" | "review" | "reconciled"
  match_reason: string
  changes: Array<{
    field: "result" | "deck" | "kills" | "notes"
    player: string | null
    before: string | number | null
    after: string | number | null
  }>
  imported_id: number | null
  candidates: SheetCandidate[]
  target: SheetCandidate | null
}

export function changeGroup(row: SheetRow) {
  if (row.status !== "changed") return row.status
  return row.changes.some((change) => change.field === "result" || change.field === "deck")
    ? "corrections"
    : "details"
}

export interface SheetPreview {
  rows: SheetRow[]
  players: Array<{ id: number; name: string }>
  decks: Array<{ id: number; player_id: number; name: string; commander_name: string }>
  revision: string
  valid: boolean
}

export const emptySheetInput = (): SheetInput => ({
  text: "",
  players: {},
  decks: {},
  actions: {},
})

export const previewSheet = (input: SheetInput) =>
  api<{ data: SheetPreview }>("/api/imports/sheet/preview", {
    method: "POST",
    body: JSON.stringify(input),
  }).then((body) => body.data)

export const commitSheet = (input: SheetInput & { revision: string }) =>
  api<{ data: { created: number; updated: number; skipped: number; game_ids: number[] } }>(
    "/api/imports/sheet",
    { method: "POST", body: JSON.stringify(input) },
  ).then((body) => body.data)
