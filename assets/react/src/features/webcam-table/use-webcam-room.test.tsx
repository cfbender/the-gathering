import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { EMPTY_TURNS } from "./turns"
import { TOKEN_REFRESH_INTERVAL_MS } from "./use-room-channel"
import { wire } from "./test-support/fake-phoenix"
import {
  FakePeerConnection,
  fakeMedia,
  installFakeMedia,
  installFakeWebRtc,
  serveTableConfig,
} from "./test-support/fake-webrtc"
import {
  CAPTURE_TIMEOUT_MS,
  useWebcamRoom,
  type BoardCard,
  type TableParticipant,
} from "./use-webcam-room"

const camera = vi.hoisted(() => ({ open: vi.fn() }))
vi.mock("phoenix", () => import("./test-support/fake-phoenix"))
vi.mock("./camera", () => ({ openCamera: () => camera.open() }))

beforeEach(() => {
  wire.reset()
  camera.open.mockResolvedValue(fakeMedia().media)
  installFakeMedia()
  installFakeWebRtc()
  serveTableConfig()
})
afterEach(() => {
  vi.useRealTimers()
  cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  camera.open.mockReset()
})

function renderRoom() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return renderHook(() => useWebcamRoom("room", 7, null), {
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  })
}

async function joinedRoom(participant: TableParticipant = saved) {
  const view = renderRoom()
  await waitFor(() => expect(wire.channel).not.toBeNull())
  await act(async () => wire.channel!.joinPush.reply("ok", { participant }))
  return view
}

function payloads(event: string) {
  return wire.sent(event).map(({ payload }) => payload)
}

const saved: TableParticipant = {
  peer_id: "restored",
  player_id: 7,
  player_name: "Cody",
  joined_at: 12,
  life: 23,
  poison: 6,
  rad: 2,
  commander_casts: { Kangee: 3 },
  commander_damage: { 19: { Atraxa: 11 } },
  camera_off: false,
  eliminated: false,
}

function tableState(overrides: Record<string, unknown> = {}) {
  return {
    timer: { started_at: 1000, paused_at: null, paused_ms: 0, server_now: 1000 },
    peer_ids: [],
    seats: [],
    eliminated_seats: [],
    turns: EMPTY_TURNS,
    auto_randomize: false,
    owner_id: 7,
    mode: "commander",
    team_life: {},
    monarch: { holder: null, revision: 0 },
    cards: [],
    ...overrides,
  }
}

function boardCard(id: string, ownerPeerId: string, name: string): BoardCard {
  return { id, ownerPeerId, byPlayerName: "Theo", at: 1, card: { id, name, set: "lea" } }
}

it("hydrates before editing and reconnects without republishing default life or counters", async () => {
  const { result } = await joinedRoom()
  expect(result.current.life).toBe(23)
  expect(result.current.counters.commander_damage).toEqual({ 19: { Atraxa: 11 } })
  expect(payloads("update_status")).toContainEqual({ camera_off: false })
  expect(payloads("update_status")).not.toContainEqual(expect.objectContaining({ life: 40 }))
  act(() => result.current.changeLife(-2))
  expect(payloads("update_status").at(-1)).toEqual({ life: 21 })
  act(() => wire.socketError())
  expect(result.current.status).toMatch(/Reconnecting/)
  const beforeRetry = wire.channel!.params()
  act(() => wire.channel!.fail())
  expect(wire.channel!.params().peer_id).not.toBe(beforeRetry.peer_id)
  expect(wire.channel!.params().peer_id).toBe(result.current.peerId)
  expect(wire.channel!.params().player_id).toBe(beforeRetry.player_id)
  await act(async () => wire.channel!.joinPush.reply("ok", { participant: { ...saved, life: 21 } }))
  expect(result.current.life).toBe(21)
  expect(result.current.error).toBeNull()
  expect(camera.open).toHaveBeenCalledOnce()
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))
  expect(wire.socketParams().token).toBe("fresh-token")
})

it("refreshes the socket token at most once per interval while reconnect attempts keep failing", async () => {
  let now = 1_000_000
  vi.spyOn(Date, "now").mockImplementation(() => now)
  await joinedRoom()
  expect(fetch).toHaveBeenCalledTimes(1)

  act(() => wire.socketError())
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))

  // phoenix.js retries every few seconds during an outage; those attempts reuse the fresh token.
  for (let attempt = 0; attempt < 10; attempt += 1) {
    now += 2_000
    act(() => wire.socketError())
  }
  await act(async () => {})
  expect(fetch).toHaveBeenCalledTimes(2)

  now += TOKEN_REFRESH_INTERVAL_MS
  act(() => wire.socketError())
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(3))
})

it("hydrates team state and routes life shortcuts to the viewer's team without changing personal counters", async () => {
  const { result } = await joinedRoom()
  const seats = [12, 19, 3, 7].map((id) => ({ ...saved, player_id: id, peer_id: `peer-${id}` }))
  act(() =>
    wire.channel!.emit(
      "table_state",
      tableState({
        peer_ids: seats.map((seat) => seat.peer_id),
        seats,
        owner_id: 12,
        mode: "two_headed_giant",
        team_life: { 0: 56, 1: 61 },
      }),
    ),
  )
  expect(result.current.mode).toBe("two_headed_giant")
  expect(result.current.teamLife).toEqual({ 0: 56, 1: 61 })
  act(() => result.current.changeLife(-10))
  expect(wire.pushes.at(-1)).toMatchObject({
    event: "adjust_team_life",
    payload: { team_index: 1, delta: -10 },
  })
  expect(result.current.life).toBe(23)
  expect(result.current.counters.poison).toBe(6)
})

it("takes table controls from the join reply, not the room's creator id", async () => {
  const { result } = await joinedRoom()
  // The fixture's owner_id matches this seat, but only the server's join decision counts.
  act(() => wire.channel!.emit("table_state", tableState()))
  expect(result.current.isOwner).toBe(false)

  await act(async () => wire.channel!.joinPush.reply("ok", { participant: saved, owner: true }))
  expect(result.current.isOwner).toBe(true)
})

it("late spectators never request a camera or publish life/counters", async () => {
  const { result } = await joinedRoom({ ...saved, spectator: true })
  expect(result.current.spectating).toBe(true)
  expect(result.current.status).toMatch(/Spectating/)
  expect(camera.open).not.toHaveBeenCalled()
  act(() => {
    result.current.changeLife(-1)
    result.current.adjustCounter({ kind: "poison" }, 1)
  })
  expect(payloads("update_status")).toEqual([])
})

it("lists spectators from presence apart from the seats", async () => {
  const { result } = await joinedRoom()
  const self = { ...saved, peer_id: result.current.peerId }
  const watchers = [
    { ...saved, player_id: 11, player_name: "Wren", peer_id: "zz-wren", spectator: true },
    { ...saved, player_id: 12, player_name: "Ada", peer_id: "zz-ada", spectator: true },
  ]
  act(() => wire.presence!.sync([self, ...watchers]))
  expect(result.current.participants.map((seat) => seat.player_name)).toEqual(["Cody"])
  expect(result.current.spectators.map((watcher) => watcher.player_name)).toEqual(["Ada", "Wren"])

  act(() => wire.presence!.sync([self]))
  expect(result.current.spectators).toEqual([])
})

it("keeps the hidden capture video playing after the camera replaces the placeholder", async () => {
  // Swapping srcObject pauses a media element; a paused capture video would hand every
  // click the same frozen first frame instead of what is on the table now.
  const { media } = fakeMedia()
  camera.open.mockResolvedValue(media)
  const playedSources: unknown[] = []
  vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(function (
    this: HTMLVideoElement,
  ) {
    playedSources.push(this.srcObject)
    return Promise.resolve()
  })
  await joinedRoom()
  await waitFor(() => expect(camera.open).toHaveBeenCalledOnce())
  await waitFor(() => expect(playedSources).toContain(media))
})

it("joins with only the seat identity; the server no longer negotiates a protocol version", async () => {
  const { result } = await joinedRoom()
  expect(wire.channel!.params()).toEqual({
    peer_id: result.current.peerId,
    player_id: 7,
    deck_id: null,
  })
})

it("shows the server's card list, overlaying only changes the server has not answered", async () => {
  const { result } = await joinedRoom()
  const theirs = boardCard("theirs", "remote", "Counterspell")
  act(() => wire.channel!.emit("table_state", tableState({ cards: [theirs] })))
  expect(result.current.identifiedCards).toEqual([theirs])

  const bolt = { id: "bolt", name: "Lightning Bolt", set: "lea" }
  let entry!: BoardCard
  act(() => {
    entry = result.current.announceCard(result.current.peerId, "Cody", bolt)
  })
  // The server stamps the identifier from the sender's seat; only the local overlay names it.
  const { byPlayerName: _stamped, ...pushed } = entry
  expect(wire.sent("cards").at(-1)?.payload).toEqual({ type: "card_identified", entry: pushed })
  expect(result.current.identifiedCards).toEqual([theirs, entry])
  act(() => wire.sent("cards").at(-1)!.push.reply("error", { reason: "invalid cards" }))
  expect(result.current.identifiedCards).toEqual([theirs])

  act(() => {
    entry = result.current.announceCard(result.current.peerId, "Cody", bolt)
  })
  act(() => {
    wire.channel!.emit("identified_cards", { entries: [theirs, entry] })
    wire.sent("cards").at(-1)!.push.reply("ok")
  })
  expect(result.current.identifiedCards).toEqual([theirs, entry])

  act(() => result.current.clearOwnCards())
  expect(wire.sent("cards").at(-1)?.payload).toEqual({
    type: "cards_cleared",
    ownerPeerId: result.current.peerId,
  })
  expect(result.current.identifiedCards).toEqual([theirs])
  act(() => wire.sent("cards").at(-1)!.push.reply("timeout"))
  expect(result.current.identifiedCards).toEqual([theirs, entry])

  // Another seat's accepted change arrives as the whole list.
  act(() => wire.channel!.emit("identified_cards", { entries: [] }))
  expect(result.current.identifiedCards).toEqual([])
})

it("prefetches details and images for every card new to this seat, newest first, one at a time", async () => {
  const preloaded: string[] = []
  vi.stubGlobal(
    "Image",
    class {
      fetchPriority = ""
      decoding = ""
      set src(value: string) {
        preloaded.push(value)
      }
    },
  )
  const fetch = vi.mocked(globalThis.fetch)
  const serveConfig = fetch.getMockImplementation()!
  const answers = new Map<string, () => void>()
  fetch.mockImplementation(async (input, init) => {
    const url = input instanceof Request ? input.url : input.toString()
    const id = url.match(/\/api\/card-printings\/([^/]+)\/details$/)?.[1]
    if (!id) return serveConfig(input, init)
    await new Promise<void>((resolve) => answers.set(id, resolve))
    return new Response(
      JSON.stringify({
        data: { id, image_uris: { small: `/small/${id}`, normal: `/normal/${id}` } },
      }),
    )
  })
  const requested = () =>
    fetch.mock.calls
      .map(([input]) => (input instanceof Request ? input.url : input.toString()))
      .flatMap((url) => url.match(/card-printings\/([^/]+)\/details$/)?.[1] ?? [])
  const answer = async (id: string) => {
    await waitFor(() => expect(answers.has(id)).toBe(true))
    await act(async () => answers.get(id)!())
  }

  await joinedRoom()
  // Joining mid-game: the table already has cards, fetched newest first and one at a time.
  const older = boardCard("older", "remote", "Counterspell")
  const newer = boardCard("newer", "remote", "Swords to Plowshares")
  act(() => wire.channel!.emit("table_state", tableState({ cards: [older, newer] })))
  await waitFor(() => expect(requested()).toEqual(["newer"]))
  await answer("newer")
  await waitFor(() => expect(requested()).toEqual(["newer", "older"]))
  await answer("older")
  await waitFor(() =>
    expect(preloaded).toEqual(["/small/newer", "/normal/newer", "/small/older", "/normal/older"]),
  )

  // Another seat names a card; the same printing on a second board is fetched once.
  const named = boardCard("named", "remote", "Lightning Bolt")
  const sameOnAnotherBoard = { ...named, id: "again", ownerPeerId: "other" }
  act(() =>
    wire.channel!.emit("identified_cards", {
      entries: [older, newer, named, sameOnAnotherBoard],
    }),
  )
  await answer("named")
  await waitFor(() => expect(preloaded.slice(4)).toEqual(["/small/named", "/normal/named"]))

  // A later snapshot with nothing new fetches nothing.
  act(() => wire.channel!.emit("table_state", tableState({ cards: [older, newer, named] })))
  expect(requested()).toEqual(["newer", "older", "named"])
})

it("ignores card messages from peers and never sends cards over data channels", async () => {
  const { result } = await joinedRoom()
  const theirs = boardCard("theirs", "remote", "Counterspell")
  act(() => wire.channel!.emit("identified_cards", { entries: [theirs] }))
  const remote = { ...saved, player_id: 9, player_name: "Theo", peer_id: "zz-remote" }
  act(() => wire.presence!.sync([{ ...saved, peer_id: result.current.peerId }, remote]))
  // The lower peer ID offers and opens the data channel.
  const channel = FakePeerConnection.instances[0]!.channels[0]!
  const injected = boardCard("injected", result.current.peerId, "Black Lotus")
  act(() => {
    channel.deliver(JSON.stringify({ type: "card_identified", entry: injected }))
    channel.deliver(JSON.stringify({ type: "cards_sync", entries: [injected] }))
    channel.deliver(JSON.stringify({ type: "card_removed", id: "theirs" }))
    channel.deliver(JSON.stringify({ type: "cards_cleared", ownerPeerId: "remote" }))
  })
  expect(result.current.identifiedCards).toEqual([theirs])

  act(() => {
    result.current.announceCard(result.current.peerId, "Cody", {
      id: "bolt",
      name: "Lightning Bolt",
      set: "lea",
    })
    result.current.clearOwnCards()
  })
  expect(channel.sent).toEqual([])
})

const theo = { ...saved, player_id: 9, player_name: "Theo", peer_id: "zz-remote" }

/** Seats Cody (this hook) and Theo, whose higher peer ID makes this side open the channel. */
async function roomWithTheo() {
  const view = await joinedRoom()
  const self = { ...saved, peer_id: view.result.current.peerId }
  act(() => wire.presence!.sync([self, theo]))
  const channel = FakePeerConnection.instances[0]!.channels[0]!
  return { ...view, self, channel }
}

/** Lets each peer's serialized sender updates run to completion. */
async function flushSenderUpdates() {
  await act(async () => new Promise((resolve) => setTimeout(resolve, 0)))
}

it("leaves video senders alone when a presence sync changes nothing they send", async () => {
  // Chrome resets a sender's encoder on every replaceTrack, even with the same track, and
  // each life tap at the table is a presence sync; re-attaching stalled every camera.
  const { result, self } = await roomWithTheo()
  const sender = FakePeerConnection.instances[0]!.senders[0]!
  await flushSenderUpdates()
  expect(sender.setParameters).toHaveBeenCalledOnce()
  sender.replaceTrack.mockClear()
  sender.setParameters.mockClear()

  for (const life of [39, 38, 37]) act(() => wire.presence!.sync([self, { ...theo, life }]))
  await flushSenderUpdates()
  expect(sender.replaceTrack).not.toHaveBeenCalled()
  expect(sender.setParameters).not.toHaveBeenCalled()

  // A private reveal to someone else still detaches Theo's video.
  const lee = { ...saved, player_id: 11, player_name: "Lee", peer_id: "zzz-lee" }
  act(() => wire.presence!.sync([self, theo, lee]))
  wire.onPush = (event, _payload, push) => {
    if (event === "reveal") push.reply("ok")
  }
  await act(() => result.current.changeReveal(lee.peer_id))
  expect(sender.replaceTrack).toHaveBeenCalledExactlyOnceWith(null)
})

const crop = {
  type: "capture_response",
  image: "data:image/jpeg;base64,/9j/4AAQ",
  nativeWidth: 1920,
  nativeHeight: 1080,
  cropSize: 640,
  clickX: 320,
  clickY: 320,
  private: false,
  shareCorrections: true,
}

it("drops malformed or unexpected data-channel messages without throwing", async () => {
  const { result, channel } = await roomWithTheo()
  act(() => {
    channel.deliver("{not json")
    channel.deliver(new ArrayBuffer(8))
    channel.deliver(JSON.stringify({ type: "capture_request", requestId: "r", x: 4, y: 0.5 }))
    channel.deliver(JSON.stringify({ ...crop, requestId: "never-requested" }))
  })
  expect(channel.sent).toEqual([])
  expect(result.current.capture).toBeNull()

  act(() =>
    channel.deliver(JSON.stringify({ type: "capture_request", requestId: "r", x: 0.5, y: 0.5 })),
  )
  expect(channel.messages()).toEqual([
    expect.objectContaining({ type: "capture_response", requestId: "r", private: false }),
  ])
})

it("accepts only the requested peer's well-formed crop and restores the live status", async () => {
  const { result, channel } = await roomWithTheo()
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5, true))
  expect(result.current.status).toBe("Requesting native camera crop…")
  const { requestId } = channel.messages()[0]!
  act(() => channel.deliver(JSON.stringify({ ...crop, requestId, image: "data:text/html,hi" })))
  expect(result.current.capture).toBeNull()
  act(() => channel.deliver(JSON.stringify({ ...crop, requestId, extra: "dropped" })))
  expect(result.current.capture).toEqual({
    peerId: theo.peer_id,
    playerId: 9,
    inspect: true,
    image: crop.image,
    nativeWidth: 1920,
    nativeHeight: 1080,
    cropSize: 640,
    clickX: 320,
    clickY: 320,
    private: false,
    shareCorrections: true,
  })
  expect(result.current.status).toMatch(/^Live/)
})

it("mirrors a remote crop to match the clicker's flip of that board", async () => {
  const { result, channel } = await roomWithTheo()
  const context = { setTransform: vi.fn(), drawImage: vi.fn() }
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    context as unknown as CanvasRenderingContext2D,
  )
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/jpeg;base64,seen")
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: () => Promise.resolve(),
  })
  const flip = { vertical: true, horizontal: true }
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5, false, flip))
  // The owner is asked for native pixels; the flip never leaves the clicker.
  expect(channel.messages()[0]).toEqual({
    type: "capture_request",
    requestId: expect.any(String),
    x: 0.5,
    y: 0.5,
  })
  const { requestId } = channel.messages()[0]!
  act(() => channel.deliver(JSON.stringify({ ...crop, requestId, clickX: 100, clickY: 200 })))
  await waitFor(() =>
    expect(result.current.capture).toMatchObject({
      image: "data:image/jpeg;base64,seen",
      clickX: 540,
      clickY: 440,
    }),
  )
  expect(context.setTransform).toHaveBeenCalledWith(-1, 0, 0, -1, 640, 640)
})

it("times out a crop request that a silent peer never answers", async () => {
  const { result, channel } = await roomWithTheo()
  vi.useFakeTimers()
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5))
  const { requestId } = channel.messages()[0]!
  act(() => {
    vi.advanceTimersByTime(CAPTURE_TIMEOUT_MS)
  })
  expect(result.current.status).toBe("Theo's camera did not send a crop; click the card again")
  act(() => channel.deliver(JSON.stringify({ ...crop, requestId })))
  expect(result.current.capture).toBeNull()
})

it("cancels pending crops when their peer leaves or the room unmounts", async () => {
  const { result, self, unmount } = await roomWithTheo()
  vi.useFakeTimers()
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5))
  expect(vi.getTimerCount()).toBe(1)
  act(() => wire.presence!.sync([self]))
  expect(vi.getTimerCount()).toBe(0)
  expect(result.current.status).toMatch(/^Live/)

  act(() => wire.presence!.sync([self, theo]))
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5))
  expect(vi.getTimerCount()).toBe(1)
  unmount()
  expect(vi.getTimerCount()).toBe(0)
})

it("logs failed negotiation steps instead of leaving unhandled rejections", async () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {})
  FakePeerConnection.onCreate = (connection) => {
    connection.createOffer.mockRejectedValue(new DOMException("no codecs", "OperationError"))
    connection.setRemoteDescription.mockRejectedValue(
      new DOMException("bad sdp", "InvalidStateError"),
    )
  }
  const { result } = await roomWithTheo()
  await waitFor(() =>
    expect(warn).toHaveBeenCalledWith(
      "WebRTC negotiation with zz-remote failed",
      expect.anything(),
    ),
  )

  // An offer from a lower peer ID: its failure is logged and later steps still run in order.
  const target = result.current.peerId
  act(() => {
    wire.channel!.emit("signal", {
      target,
      from: "00-remote",
      signal: { description: { type: "offer", sdp: "o" } },
    })
    wire.channel!.emit("signal", {
      target,
      from: "00-remote",
      signal: { candidate: { candidate: "c" } },
    })
  })
  await waitFor(() =>
    expect(warn).toHaveBeenCalledWith(
      "WebRTC negotiation with 00-remote failed",
      expect.anything(),
    ),
  )
  const answering = FakePeerConnection.instances[1]!
  expect(answering.createAnswer).not.toHaveBeenCalled()
  expect(wire.sent("signal")).toEqual([])
})

it("abandons an offer when its peer leaves mid-negotiation", async () => {
  let resolveOffer!: (offer: RTCSessionDescriptionInit) => void
  FakePeerConnection.onCreate = (connection) => {
    connection.createOffer.mockReturnValue(
      new Promise((resolve) => {
        resolveOffer = resolve
      }),
    )
  }
  const { self } = await roomWithTheo()
  const connection = FakePeerConnection.instances[0]!
  await waitFor(() => expect(connection.createOffer).toHaveBeenCalled())
  act(() => wire.presence!.sync([self]))
  await act(async () => {
    resolveOffer({ type: "offer", sdp: "late" })
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  expect(connection.setLocalDescription).not.toHaveBeenCalled()
  expect(wire.sent("signal")).toEqual([])
})
