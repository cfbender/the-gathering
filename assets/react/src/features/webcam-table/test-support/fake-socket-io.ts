// A stand-in for the `socket.io-client` module in webcam-table tests. Load it with
// `vi.mock("socket.io-client", () => import("./test-support/fake-socket-io"))` and drive the
// table through `wire`: join replies, server events, presence, and push replies.
import { applyCardCommand, type CardCommand } from "../identified-cards"
import type { BoardCard, TableParticipant } from "../use-webcam-room"

type Listener = (...args: unknown[]) => void
type Status = "ok" | "error" | "timeout"
type Ack = (error: Error | null, reply?: unknown) => void

/** An emitted event whose acknowledgement the test sends, as the server would. */
export class FakePush {
  constructor(private readonly ack: Ack | undefined) {}

  reply(status: Status, payload: unknown = {}) {
    if (status === "timeout") this.ack?.(new Error("operation has timed out"))
    else if (status === "ok") this.ack?.(null, { ok: payload })
    else this.ack?.(null, { error: (payload as { reason: string }).reason })
  }
}

interface Options {
  auth: (send: (auth: Record<string, unknown>) => void) => void
}

export class FakeSocket {
  connected = true
  active = true
  private readonly listeners = new Map<string, Listener[]>()

  constructor(readonly options: Options) {}

  on(event: string, callback: Listener) {
    this.listeners.set(event, [...(this.listeners.get(event) ?? []), callback])
    return this
  }

  /** Delivers an event to every listener, like a server emit or a socket lifecycle event. */
  fire(event: string, ...args: unknown[]) {
    for (const listener of this.listeners.get(event) ?? []) listener(...args)
  }

  timeout(_ms: number) {
    return { emit: (event: string, payload: unknown, ack: Ack) => this.record(event, payload, ack) }
  }

  emit(event: string, payload: unknown, ack?: Ack) {
    this.record(event, payload, ack)
    return this
  }

  private record(event: string, payload: unknown, ack: Ack | undefined) {
    const push = new FakePush(ack)
    wire.pushes.push({ event, payload, push })
    wire.onPush?.(event, payload, push)
  }

  connect() {
    wire.connects += 1
    this.active = true
    return this
  }

  disconnect() {
    this.connected = false
    this.active = false
    return this
  }
}

export function io(options: Options) {
  wire.socket = new FakeSocket(options)
  return wire.socket
}

export const wire = {
  socket: null as FakeSocket | null,
  pushes: [] as { event: string; payload: unknown; push: FakePush }[],
  onPush: null as null | ((event: string, payload: unknown, push: FakePush) => void),
  connects: 0,
  reset() {
    wire.socket = null
    wire.pushes = []
    wire.onPush = null
    wire.connects = 0
  },
  /** Pushes of one event, newest last. */
  sent(event: string) {
    return wire.pushes.filter((push) => push.event === event)
  },
  /** The latest join attempt. */
  joinPush() {
    return wire.sent("join").at(-1)!.push
  },
  /** The latest join attempt's params. */
  joinParams() {
    return wire.sent("join").at(-1)!.payload as Record<string, unknown>
  },
  /** A server event. */
  emit(event: string, payload: unknown) {
    wire.socket!.fire(event, payload)
  },
  /** Replaces who is at the table. */
  presence(metas: TableParticipant[]) {
    wire.emit("presence", metas)
  },
  /** A failed connection attempt; `refused` when the server rejected the token. */
  socketError(refused = false) {
    wire.socket!.active = !refused
    wire.socket!.fire("connect_error", new Error(refused ? "unauthorized" : "websocket error"))
  },
  /** The connection drops and comes back. */
  reconnect() {
    wire.socket!.connected = false
    wire.socket!.fire("disconnect", "transport close")
    wire.socket!.connected = true
    wire.socket!.fire("connect")
  },
  /** The auth payload the next connection attempt sends. */
  socketParams() {
    let auth: Record<string, unknown> = {}
    wire.socket!.options.auth((value) => {
      auth = value
    })
    return auth
  },
}

/** Answers `cards` pushes like the server: apply, broadcast the whole list, then reply ok. */
export function serveCards(initial: BoardCard[] = []) {
  let entries = initial
  wire.onPush = (event, payload, push) => {
    if (event !== "cards") return
    entries = applyCardCommand(entries, payload as CardCommand)
    wire.emit("identified_cards", { entries })
    push.reply("ok")
  }
}
