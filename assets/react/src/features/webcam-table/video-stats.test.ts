import { expect, it } from "vite-plus/test"
import { summarizeVideoStats } from "./video-stats"

function report(bytes = 95_000, timestamp = 3500): RTCStatsReport {
  return new Map([
    [
      "video",
      {
        id: "video",
        type: "inbound-rtp",
        kind: "video",
        bytesReceived: bytes,
        timestamp,
        frameWidth: 960,
        frameHeight: 540,
        framesPerSecond: 24,
      },
    ],
    [
      "audio",
      { id: "audio", type: "inbound-rtp", kind: "audio", bytesReceived: 999_999, timestamp },
    ],
    ["transport", { type: "transport", selectedCandidatePairId: "chosen" }],
    ["chosen", { type: "candidate-pair", remoteCandidateId: "relay" }],
    ["relay", { type: "remote-candidate", candidateType: "relay" }],
    ["unused", { type: "remote-candidate", candidateType: "host" }],
  ])
}

it("uses only video byte deltas and the selected ICE path", () => {
  expect(
    summarizeVideoStats(report(), { id: "video", bytes: 20_000, timestamp: 1500 }),
  ).toMatchObject({ width: 960, height: 540, fps: 24, kbps: 300, candidate: "relay" })
  expect(summarizeVideoStats(report()).kbps).toBeUndefined()
  expect(
    summarizeVideoStats(report(95_000, 5500), { id: "video", bytes: 95_000, timestamp: 3500 }).kbps,
  ).toBe(0)
})

it("resets rate sampling after replacement, counter resets or nonincreasing timestamps", () => {
  for (const previous of [
    { id: "old", bytes: 0, timestamp: 1500 },
    { id: "video", bytes: 100_000, timestamp: 1500 },
    { id: "video", bytes: 20_000, timestamp: 3500 },
    { id: "video", bytes: 20_000, timestamp: 4500 },
  ])
    expect(summarizeVideoStats(report(), previous).kbps).toBeUndefined()
  expect(summarizeVideoStats(new Map())).toEqual({ candidate: undefined })
})
