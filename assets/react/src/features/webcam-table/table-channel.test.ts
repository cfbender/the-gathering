import type { Socket } from "socket.io-client"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { PUSH_TIMEOUT_MS, TableChannel } from "./table-channel"
import { FakeSocket, wire } from "./test-support/fake-socket-io"

let socket: FakeSocket
let generation = 0

function channel() {
  const room = new TableChannel(socket as unknown as Socket, () => ({
    peer_id: `peer-${generation}`,
  }))
  room.onError(() => {
    generation += 1
  })
  return room
}

beforeEach(() => {
  vi.useFakeTimers()
  wire.reset()
  generation = 0
  socket = new FakeSocket({ auth: (send) => send({}) })
  wire.socket = socket
})
afterEach(() => {
  vi.useRealTimers()
})

it("holds pushes until the join succeeds and maps acknowledgements to replies", () => {
  const room = channel()
  const replies: unknown[] = []
  room.join()
  room.push("update_status", { life: 39 }).receive("ok", (response) => replies.push(response))
  expect(wire.sent("update_status")).toEqual([])

  wire.joinPush().reply("ok", { participant: {} })
  expect(room.state).toBe("joined")
  wire.sent("update_status")[0]!.push.reply("ok", { applied: true })
  room.push("roll", { kind: "coin" }).receive("error", (response) => replies.push(response))
  wire.sent("roll")[0]!.push.reply("error", { reason: "rate limited" })
  room.push("timer", {}).receive("timeout", () => replies.push("timed out"))
  wire.sent("timer")[0]!.push.reply("timeout")
  expect(replies).toEqual([{ applied: true }, { reason: "rate limited" }, "timed out"])
})

it("times out a push that never gets to be sent", () => {
  const room = channel()
  const timeout = vi.fn()
  room.join()
  room.push("update_status", { life: 39 }).receive("timeout", timeout)
  vi.advanceTimersByTime(PUSH_TIMEOUT_MS)
  expect(timeout).toHaveBeenCalledOnce()
  wire.joinPush().reply("ok", {})
  expect(wire.sent("update_status")).toEqual([])
})

it("rejoins with fresh params after the server drops the seat, backing off on refusals", () => {
  const room = channel()
  const joined = vi.fn()
  room.join().receive("ok", joined)
  wire.joinPush().reply("ok", { owner: false })

  wire.emit("rejoin", { reason: "room_down" })
  expect(room.state).toBe("errored")
  expect(wire.sent("join")).toHaveLength(1)
  vi.advanceTimersByTime(1_000)
  expect(wire.joinParams()).toEqual({ peer_id: "peer-1" })

  wire.joinPush().reply("error", { reason: "rate limited" })
  vi.advanceTimersByTime(1_999)
  expect(wire.sent("join")).toHaveLength(2)
  vi.advanceTimersByTime(1)
  expect(wire.sent("join")).toHaveLength(3)
  wire.joinPush().reply("ok", { owner: true })
  expect(joined).toHaveBeenLastCalledWith({ owner: true })
  expect(room.state).toBe("joined")
})

it("joins again on reconnect and ignores replies to superseded attempts", () => {
  const room = channel()
  const joined = vi.fn()
  room.join().receive("ok", joined)
  const first = wire.joinPush()

  wire.reconnect()
  expect(wire.joinParams()).toEqual({ peer_id: "peer-1" })
  first.reply("ok", { stale: true })
  expect(joined).not.toHaveBeenCalled()
  wire.joinPush().reply("ok", { stale: false })
  expect(joined).toHaveBeenCalledExactlyOnceWith({ stale: false })
})

it("stops rejoining once left", () => {
  const room = channel()
  room.join()
  wire.joinPush().reply("ok", {})
  room.leave()
  expect(wire.sent("leave")).toHaveLength(1)
  wire.emit("rejoin", { reason: "room_down" })
  wire.reconnect()
  vi.advanceTimersByTime(10_000)
  expect(wire.sent("join")).toHaveLength(1)
})
