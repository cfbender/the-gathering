import { describe, expect, it } from "vite-plus/test"
import { describeIceServers } from "./side-panel-labels"
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
