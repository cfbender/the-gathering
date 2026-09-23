import { afterEach, expect, it, vi } from "vite-plus/test"
import { openCamera, videoHealth } from "./camera"

afterEach(() => vi.unstubAllGlobals())

it("selects the exact requested camera but uses ideal resolution without capturing audio", async () => {
  const getUserMedia = vi.fn().mockResolvedValue({})
  vi.stubGlobal("navigator", { mediaDevices: { getUserMedia } })
  await openCamera("second-camera")
  expect(getUserMedia).toHaveBeenLastCalledWith({
    video: {
      deviceId: { exact: "second-camera" },
      width: { ideal: 1920 },
      height: { ideal: 1080 },
      frameRate: { ideal: 30 },
    },
    audio: false,
  })
  await openCamera("")
  expect(getUserMedia.mock.lastCall?.[0].video).not.toHaveProperty("deviceId")
})

it("reports absent, disabled, ended and muted tracks rather than promising healthy video", () => {
  expect(videoHealth(null)).toContain("No camera track")
  const stream = {
    getVideoTracks: () => [
      {
        label: "Desk cam",
        readyState: "ended",
        enabled: false,
        muted: true,
        getSettings: () => ({ width: 1280, height: 720, frameRate: 24 }),
      },
    ],
  } as unknown as MediaStream
  expect(videoHealth(stream)).toBe(
    "Desk cam: 1280 × 720, 24 fps. Track ended; disabled, source muted.",
  )
})
