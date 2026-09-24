// A stand-in for the `phoenix` module in webcam-table tests. Load it with
// `vi.mock("phoenix", () => import("./test-support/fake-phoenix"))` and drive the room
// through `wire`: join replies, server broadcasts, presence, and push replies.
import { applyCardCommand, type CardCommand } from "../identified-cards"
import type { BoardCard, TableParticipant } from "../use-webcam-room"

type Listener = (payload: unknown) => void
type Status = "ok" | "error" | "timeout"

/** A push whose reply the test sends. A reply that arrives before `receive` is registered
 * is replayed, like Phoenix does for pushes that already have a response. */
export class FakePush {
  private readonly handlers = new Map<Status, Listener>()
  private result: { status: Status; payload: unknown } | null = null

  receive(status: Status, callback: Listener) {
    this.handlers.set(status, callback)
    if (this.result?.status === status) callback(this.result.payload)
    return this
  }

  reply(status: Status, payload: unknown = {}) {
    this.result = { status, payload }
    this.handlers.get(status)?.(payload)
  }
}

export class FakeChannel {
  state = "joined"
  readonly joinPush = new FakePush()
  private readonly listeners = new Map<string, Listener[]>()
  private errorCallback = () => {}

  constructor(
    readonly topic: string,
    readonly params: () => Record<string, unknown>,
  ) {}

  on(event: string, callback: Listener) {
    this.listeners.set(event, [...(this.listeners.get(event) ?? []), callback])
    return this.listeners.get(event)!.length
  }

  onError(callback: () => void) {
    this.errorCallback = callback
  }

  /** Delivers a server broadcast to every binding for `event`. */
  emit(event: string, payload: unknown) {
    for (const listener of this.listeners.get(event) ?? []) listener(payload)
  }

  fail() {
    this.errorCallback()
  }

  push(event: string, payload: unknown) {
    const push = new FakePush()
    wire.pushes.push({ event, payload, push })
    wire.onPush?.(event, payload, push)
    return push
  }

  join() {
    return this.joinPush
  }

  leave() {
    this.state = "closed"
  }
}

export class Presence {
  private metas: TableParticipant[] = []
  private syncCallback = () => {}

  constructor(_channel: FakeChannel) {
    wire.presence = this
  }

  onJoin() {}
  onLeave() {}

  onSync(callback: () => void) {
    this.syncCallback = callback
  }

  list<T>(chooser: (id: string, value: { metas: TableParticipant[] }) => T) {
    return this.metas.map((meta) => chooser(meta.peer_id, { metas: [meta] }))
  }

  /** Replaces who is at the table and fires the hook's sync handler. */
  sync(metas: TableParticipant[]) {
    this.metas = metas
    this.syncCallback()
  }
}

export class Socket {
  constructor(_url: string, options: { params: () => Record<string, unknown> }) {
    wire.socketParams = options.params
  }
  connect() {}
  disconnect() {}
  onError(callback: () => void) {
    wire.socketError = callback
  }
  channel(topic: string, params: () => Record<string, unknown>) {
    wire.channel = new FakeChannel(topic, params)
    return wire.channel
  }
}

export const wire = {
  channel: null as FakeChannel | null,
  presence: null as Presence | null,
  socketParams: (() => ({})) as () => Record<string, unknown>,
  socketError: () => {},
  pushes: [] as { event: string; payload: unknown; push: FakePush }[],
  onPush: null as null | ((event: string, payload: unknown, push: FakePush) => void),
  reset() {
    wire.channel = null
    wire.presence = null
    wire.socketParams = () => ({})
    wire.socketError = () => {}
    wire.pushes = []
    wire.onPush = null
  },
  /** Pushes of one event, newest last. */
  sent(event: string) {
    return wire.pushes.filter((push) => push.event === event)
  },
}

/** Answers `cards` pushes like the server: apply, broadcast the whole list, then reply ok. */
export function serveCards(initial: BoardCard[] = []) {
  let entries = initial
  wire.onPush = (event, payload, push) => {
    if (event !== "cards") return
    entries = applyCardCommand(entries, payload as CardCommand)
    wire.channel?.emit("identified_cards", { entries })
    push.reply("ok")
  }
}
