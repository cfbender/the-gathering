import { describe, expect, it } from "vite-plus/test"
import {
  canManageDeck,
  canManageGame,
  canManagePlayer,
  type Deck,
  type Game,
  type Player,
} from "@/lib/games"

const player = (user_id: number | null): Player => ({
  id: 7,
  name: "Drew",
  avatar_url: null,
  user_id,
  archived_at: null,
})

describe("canManagePlayer", () => {
  it("lets admins manage anyone", () => {
    expect(canManagePlayer({ id: 1, role: "admin" }, player(2))).toBe(true)
  })

  it("lets members manage their own player and unclaimed guests only", () => {
    expect(canManagePlayer({ id: 2, role: "member" }, player(2))).toBe(true)
    expect(canManagePlayer({ id: 2, role: "member" }, player(null))).toBe(true)
    expect(canManagePlayer({ id: 2, role: "member" }, player(3))).toBe(false)
  })

  it("denies signed-out viewers", () => {
    expect(canManagePlayer(undefined, player(null))).toBe(false)
  })
})

const deck = (user_id: number | null): Deck => ({
  id: 3,
  player_id: 7,
  name: "Birds",
  commander_card_id: null,
  commander_name: "Kangee",
  commander_art_crop_url: null,
  partner_card_id: null,
  partner_name: null,
  partner_art_crop_url: null,
  color_identity: "WU",
  decklist_url: null,
  decklist_source: null,
  archived_at: null,
  player: player(user_id),
})

describe("canManageDeck", () => {
  it("allows administrators and the linked owner, but not members managing guest players", () => {
    expect(canManageDeck({ id: 1, role: "admin" }, deck(null))).toBe(true)
    expect(canManageDeck({ id: 2, role: "member" }, deck(2))).toBe(true)
    expect(canManageDeck({ id: 2, role: "member" }, deck(null))).toBe(false)
    expect(canManageDeck({ id: 2, role: "member" }, deck(3))).toBe(false)
  })
})

const game = (created_by_user_id: number | null, playerUserIds: (number | null)[]): Game => ({
  id: 10,
  played_at: "2026-09-20T12:00:00Z",
  duration_minutes: null,
  turns: null,
  notes: null,
  source: "manual",
  external_id: null,
  created_by_user_id,
  seats: playerUserIds.map((user_id, index) => ({
    id: index + 1,
    player_id: index + 1,
    deck_id: null,
    seat: index + 1,
    result: index === 0 ? "win" : "loss",
    mvp_card_id: null,
    mvp_card_name: null,
    mvp_art_crop_url: null,
    notes: null,
    player: { ...player(user_id), id: index + 1 },
    deck: null,
  })),
})

describe("canManageGame", () => {
  it("allows admins, creators, and seated linked players", () => {
    expect(canManageGame({ id: 1, role: "admin" }, game(2, [3, 4]))).toBe(true)
    expect(canManageGame({ id: 2, role: "member" }, game(2, [3, 4]))).toBe(true)
    expect(canManageGame({ id: 3, role: "member" }, game(2, [3, 4]))).toBe(true)
  })

  it("denies unrelated and signed-out viewers", () => {
    expect(canManageGame({ id: 5, role: "member" }, game(2, [3, null]))).toBe(false)
    expect(canManageGame(undefined, game(2, [3, 4]))).toBe(false)
  })
})
