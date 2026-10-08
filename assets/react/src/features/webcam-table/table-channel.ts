import type { Socket } from "socket.io-client"

export type ReplyStatus = "ok" | "error" | "timeout"

/** How long a push waits for its reply before it times out. */
export const PUSH_TIMEOUT_MS = 10_000
/** Signaling for one media connection; a rejoin starts a new one, so queued signals are stale. */
const SIGNAL_EVENTS = new Set([
  "sfu_offer",
  "sfu_answer",
  "sfu_candidate",
  "sfu_layer",
  "peer_message",
])
/** Delays between join attempts after the server drops the seat or refuses a join. */
const REJOIN_DELAYS_MS = [1_000, 2_000, 5_000, 10_000]

/** The server acknowledges an event with `{ ok: response }` or `{ error: reason }`. */
type Ack = { ok: unknown } | { error: string }

// Payloads are typed by each caller's callback.
type Callback = (payload: any) => void

/** A push's reply. Callbacks may be added after the reply arrived; the last reply replays. */
export class Push {
  private readonly callbacks = new Map<ReplyStatus, Callback[]>()
  private result: { status: ReplyStatus; payload: unknown } | null = null

  receive(status: ReplyStatus, callback: Callback) {
    this.callbacks.set(status, [...(this.callbacks.get(status) ?? []), callback])
    if (this.result?.status === status) callback(this.result.payload)
    return this
  }

  /** Delivers a reply to every callback for its status. */
  settle(status: ReplyStatus, payload: unknown) {
    this.result = { status, payload }
    for (const callback of this.callbacks.get(status) ?? []) callback(payload)
  }
}

function settleAck(push: Push, error: Error | null, ack: Ack | undefined) {
  if (error || !ack) push.settle("timeout", {})
  else if ("ok" in ack) push.settle("ok", ack.ok)
  else push.settle("error", { reason: ack.error })
}

type ChannelState = "closed" | "joining" | "joined" | "errored"

interface Buffered {
  event: string
  payload: unknown
  push: Push
  timer: ReturnType<typeof setTimeout>
}

/**
 * One seat's membership at a table over a Socket.IO socket. It joins whenever the socket
 * connects, rejoins after the server drops the seat (`rejoin`) or refuses a join, and holds
 * pushes made while not joined until the join succeeds. Join params are read on every attempt.
 */
export class TableChannel {
  state: ChannelState = "closed"
  /** Settles on every join attempt's reply. */
  readonly joinPush = new Push()
  private readonly errorCallbacks: ((reason: string) => void)[] = []
  private buffer: Buffered[] = []
  private attempt = 0
  private joinRef = 0
  /** Set when the seat drops; the next join drops queued signals. */
  private dropped = false
  private rejoinTimer: ReturnType<typeof setTimeout> | undefined

  constructor(
    private readonly socket: Socket,
    private readonly params: () => Record<string, unknown>,
  ) {
    socket.on("connect", () => {
      if (this.state === "closed") return
      this.attempt = 0
      this.sendJoin()
    })
    socket.on("disconnect", () => {
      if (this.state !== "closed") this.fail("disconnected")
    })
    socket.on("rejoin", ({ reason }: { reason: string }) => {
      if (this.state === "closed") return
      this.fail(reason)
      this.scheduleRejoin()
    })
  }

  /** Listens for a server event. */
  on(event: string, callback: Callback) {
    this.socket.on(event, callback)
  }

  /** Runs when the seat drops (the socket disconnected or the server asked for a rejoin). */
  onError(callback: (reason: string) => void) {
    this.errorCallbacks.push(callback)
  }

  join() {
    this.state = "joining"
    if (this.socket.connected) this.sendJoin()
    return this.joinPush
  }

  leave() {
    if (this.state !== "closed" && this.socket.connected) this.socket.emit("leave", {})
    this.state = "closed"
    clearTimeout(this.rejoinTimer)
    for (const buffered of this.buffer) clearTimeout(buffered.timer)
    this.buffer = []
  }

  push(event: string, payload: unknown) {
    const push = new Push()
    if (this.state === "joined") this.send(event, payload, push)
    else {
      const buffered: Buffered = {
        event,
        payload,
        push,
        timer: setTimeout(() => {
          this.buffer = this.buffer.filter((entry) => entry !== buffered)
          push.settle("timeout", {})
        }, PUSH_TIMEOUT_MS),
      }
      this.buffer.push(buffered)
    }
    return push
  }

  private send(event: string, payload: unknown, push: Push) {
    this.socket
      .timeout(PUSH_TIMEOUT_MS)
      .emit(event, payload, (error: Error | null, ack?: Ack) => settleAck(push, error, ack))
  }

  private sendJoin() {
    clearTimeout(this.rejoinTimer)
    this.state = "joining"
    if (this.dropped) {
      this.dropped = false
      const stale = this.buffer.filter((entry) => SIGNAL_EVENTS.has(entry.event))
      this.buffer = this.buffer.filter((entry) => !SIGNAL_EVENTS.has(entry.event))
      for (const entry of stale) {
        clearTimeout(entry.timer)
        entry.push.settle("timeout", {})
      }
    }
    const ref = ++this.joinRef
    const reply = new Push()
      .receive("ok", (response) => {
        this.state = "joined"
        this.attempt = 0
        const buffered = this.buffer
        this.buffer = []
        for (const entry of buffered) {
          clearTimeout(entry.timer)
          this.send(entry.event, entry.payload, entry.push)
        }
        this.joinPush.settle("ok", response)
      })
      .receive("error", (response) => {
        this.state = "errored"
        this.scheduleRejoin()
        this.joinPush.settle("error", response)
      })
      .receive("timeout", () => {
        this.state = "errored"
        this.scheduleRejoin()
        this.joinPush.settle("timeout", {})
      })
    this.socket
      .timeout(PUSH_TIMEOUT_MS)
      .emit("join", this.params(), (error: Error | null, ack?: Ack) => {
        // A reply to a superseded attempt (the socket reconnected meanwhile) is stale.
        if (ref === this.joinRef && this.state === "joining") settleAck(reply, error, ack)
      })
  }

  private fail(reason: string) {
    this.state = "errored"
    this.dropped = true
    this.joinRef++
    for (const callback of this.errorCallbacks) callback(reason)
  }

  private scheduleRejoin() {
    clearTimeout(this.rejoinTimer)
    const delay = REJOIN_DELAYS_MS[Math.min(this.attempt, REJOIN_DELAYS_MS.length - 1)]
    this.attempt += 1
    this.rejoinTimer = setTimeout(() => {
      if (this.state === "errored" && this.socket.connected) this.sendJoin()
    }, delay)
  }
}
