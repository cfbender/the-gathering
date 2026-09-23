import { describe, expect, it } from "vite-plus/test"
import { canViewBoard, videoEncoding } from "./media-policy"

describe("private reveal visibility", () => {
  it("allows the owner and target, but refuses third seats and late joiners", () => {
    expect(canViewBoard("alice", "alice", "bob")).toBe(true)
    expect(canViewBoard("alice", "bob", "bob")).toBe(true)
    expect(canViewBoard("alice", "cara", "bob")).toBe(false)
    expect(canViewBoard("alice", "newcomer", "bob")).toBe(false)
    expect(canViewBoard("alice", "cara", "")).toBe(false)
    expect(canViewBoard("alice", "cara", null)).toBe(true)
    expect(canViewBoard("alice", "cara", undefined)).toBe(true)
  })
})

describe("mesh sender budget", () => {
  it("overrides seat tiers without upscaling or assuming a 1080p source", () => {
    expect(videoEncoding(10, "1080p", 1080)).toEqual({
      scaleResolutionDownBy: 1,
      maxBitrate: 2_500_000,
    })
    expect(videoEncoding(2, "540p", 1080)).toEqual({
      scaleResolutionDownBy: 2,
      maxBitrate: 600_000,
    })
    expect(videoEncoding(5, "720p", 1440)).toEqual({
      scaleResolutionDownBy: 2,
      maxBitrate: 1_200_000,
    })
    expect(videoEncoding(2, "1080p", 720)).toEqual({
      scaleResolutionDownBy: 1,
      maxBitrate: 2_500_000,
    })
    expect(videoEncoding(8, "auto", 1080)).toEqual({
      scaleResolutionDownBy: 2,
      maxBitrate: 600_000,
    })
  })
  it("changes tiers at five and eight seats and restores 1080p in small rooms", () => {
    for (const size of [1, 4])
      expect(videoEncoding(size)).toEqual({ scaleResolutionDownBy: 1, maxBitrate: 2_500_000 })
    for (const size of [5, 7])
      expect(videoEncoding(size)).toEqual({ scaleResolutionDownBy: 1.5, maxBitrate: 1_200_000 })
    for (const size of [8, 10])
      expect(videoEncoding(size)).toEqual({ scaleResolutionDownBy: 2, maxBitrate: 600_000 })
  })
})
