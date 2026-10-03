import { describe, expect, it } from "vite-plus/test"
import { canViewBoard, orderVideoCodecs, videoEncoding, videoFrameRate } from "./media-policy"

describe("codec preference", () => {
  it("moves every H.264 entry to the front and keeps the rest in place", () => {
    const codecs = [
      { mimeType: "video/VP8" },
      { mimeType: "video/rtx" },
      { mimeType: "video/h264", profile: "a" },
      { mimeType: "video/AV1" },
      { mimeType: "video/H264", profile: "b" },
      { mimeType: "video/ulpfec" },
    ]
    expect(orderVideoCodecs(codecs)).toEqual([
      { mimeType: "video/h264", profile: "a" },
      { mimeType: "video/H264", profile: "b" },
      { mimeType: "video/VP8" },
      { mimeType: "video/rtx" },
      { mimeType: "video/AV1" },
      { mimeType: "video/ulpfec" },
    ])
    expect(orderVideoCodecs(codecs)).not.toBe(codecs)
  })

  it("leaves a browser without H.264 on its own order", () => {
    const codecs = [{ mimeType: "video/VP8" }, { mimeType: "video/VP9" }]
    expect(orderVideoCodecs(codecs)).toEqual(codecs)
  })
})

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

describe("top simulcast layer budget", () => {
  it("overrides seat tiers without upscaling or assuming a 1080p source", () => {
    expect(videoEncoding(10, "1080p", 1080)).toEqual({
      scaleResolutionDownBy: 1,
      maxBitrate: 2_500_000,
      maxFramerate: 15,
    })
    expect(videoEncoding(2, "540p", 1080)).toEqual({
      scaleResolutionDownBy: 2,
      maxBitrate: 600_000,
      maxFramerate: 30,
    })
    expect(videoEncoding(5, "720p", 1440)).toEqual({
      scaleResolutionDownBy: 2,
      maxBitrate: 1_200_000,
      maxFramerate: 15,
    })
    expect(videoEncoding(2, "1080p", 720)).toEqual({
      scaleResolutionDownBy: 1,
      maxBitrate: 2_500_000,
      maxFramerate: 30,
    })
    expect(videoEncoding(8, "auto", 1080)).toEqual({
      scaleResolutionDownBy: 2,
      maxBitrate: 600_000,
      maxFramerate: 15,
    })
  })
  it("changes tiers at five and eight seats and restores 1080p in small rooms", () => {
    for (const size of [1, 4])
      expect(videoEncoding(size)).toMatchObject({ scaleResolutionDownBy: 1, maxBitrate: 2_500_000 })
    for (const size of [5, 7])
      expect(videoEncoding(size)).toMatchObject({
        scaleResolutionDownBy: 1.5,
        maxBitrate: 1_200_000,
      })
    for (const size of [8, 10])
      expect(videoEncoding(size)).toMatchObject({ scaleResolutionDownBy: 2, maxBitrate: 600_000 })
  })
  it("keeps full motion for two seats and halves the frame rate from the third seat on", () => {
    expect(videoFrameRate(1)).toBe(30)
    expect(videoFrameRate(2)).toBe(30)
    expect(videoFrameRate(3)).toBe(15)
    expect(videoFrameRate(10)).toBe(15)
    // An explicit resolution ceiling does not buy the frame rate back.
    expect(videoEncoding(3, "1080p").maxFramerate).toBe(15)
    expect(videoEncoding(2, "540p").maxFramerate).toBe(30)
  })
})
