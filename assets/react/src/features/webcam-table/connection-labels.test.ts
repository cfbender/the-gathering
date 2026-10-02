import { describe, expect, it } from "vite-plus/test"
import { describeIceServers } from "./side-panel-labels"
import { describeIcePairs } from "./use-sfu-connection"
import { describeConnection } from "./use-webcam-room"

describe("describeConnection", () => {
  it("keeps the generic label until the connection actually fails", () => {
    expect(describeConnection(undefined)).toBe("Connecting…")
    expect(describeConnection("new")).toBe("Connecting…")
    expect(describeConnection("connecting")).toBe("Connecting…")
    expect(describeConnection("connected")).toBe("Connecting…")
  })

  it("distinguishes a failed peer from a transient drop and a departure", () => {
    expect(describeConnection("failed")).toBe("Couldn't connect")
    expect(describeConnection("disconnected")).toBe("Reconnecting…")
    expect(describeConnection("closed")).toBe("Left")
  })
})

describe("describeIceServers", () => {
  it("warns when there are no ICE servers at all", () => {
    expect(describeIceServers([])).toBe("no STUN or TURN — same network only")
  })

  it("counts STUN and TURN urls across string and array forms", () => {
    expect(
      describeIceServers([
        { urls: "stun:stun.l.google.com:19302" },
        { urls: ["stun:stun.cloudflare.com:3478", "turns:turn.example.com:5349"] },
      ]),
    ).toBe("2 STUN, 1 TURN")
  })

  it("flags STUN-only configurations as relay-less", () => {
    expect(describeIceServers([{ urls: ["stun:a", "stun:b"] }])).toBe("2 STUN (no relay)")
  })
})

describe("describeIcePairs", () => {
  it("names each pair's addresses, role flags and traffic from a getStats report", () => {
    const report = new Map<string, Record<string, unknown>>([
      [
        "l1",
        {
          id: "l1",
          type: "local-candidate",
          candidateType: "srflx",
          address: "203.0.113.9",
          port: 61000,
        },
      ],
      [
        "r1",
        {
          id: "r1",
          type: "remote-candidate",
          candidateType: "srflx",
          address: "198.51.100.2",
          port: 50010,
        },
      ],
      [
        "p1",
        {
          id: "p1",
          type: "candidate-pair",
          localCandidateId: "l1",
          remoteCandidateId: "r1",
          state: "succeeded",
          nominated: true,
          selected: true,
          bytesReceived: 4096,
          bytesSent: 512,
          requestsSent: 7,
          responsesReceived: 6,
          lastPacketReceivedTimestamp: 1700000000123,
        },
      ],
      [
        "p2",
        {
          id: "p2",
          type: "candidate-pair",
          localCandidateId: "l1",
          remoteCandidateId: "r-gone",
          state: "failed",
          nominated: false,
        },
      ],
      ["t", { id: "t", type: "transport" }],
    ])
    expect(describeIcePairs(report.values())).toEqual([
      "srflx 203.0.113.9:61000 -> srflx 198.51.100.2:50010 succeeded,nominated,selected rx 4096B tx 512B req 7 resp 6 last rx 1700000000123",
      "srflx 203.0.113.9:61000 -> ? failed rx 0B tx 0B req 0 resp 0 last rx never",
    ])
  })
})
