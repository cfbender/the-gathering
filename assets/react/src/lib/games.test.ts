import { describe, expect, it } from "vite-plus/test"
import { canManagePlayer, type Player } from "@/lib/games"

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
