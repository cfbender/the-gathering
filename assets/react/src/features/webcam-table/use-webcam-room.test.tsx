import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { EMPTY_TURNS } from "./turns"
import { TOKEN_REFRESH_INTERVAL_MS } from "./use-room-channel"
import { wire } from "./test-support/fake-socket-io"
import {
  FakePeerConnection,
  FakeResizeObserver,
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
vi.mock("socket.io-client", () => import("./test-support/fake-socket-io"))
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
  await waitFor(() => expect(wire.sent("join")).not.toHaveLength(0))
  await act(async () => wire.joinPush().reply("ok", { participant }))
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
  // The seat announces its camera and consent on join, then the camera's rows once it opens.
  expect(payloads("update_status")).toContainEqual({
    camera_off: false,
    camera_height: null,
    shares_corrections: true,
  })
  expect(payloads("update_status")).toContainEqual({ camera_height: 1080 })
  expect(payloads("update_status")).not.toContainEqual(expect.objectContaining({ life: 40 }))
  act(() => result.current.changeLife(-2))
  expect(payloads("update_status").at(-1)).toEqual({ life: 21 })
  act(() => wire.socketError())
  expect(result.current.status).toMatch(/Reconnecting/)
  const beforeRetry = wire.joinParams()
  act(() => wire.reconnect())
  expect(wire.joinParams().peer_id).not.toBe(beforeRetry.peer_id)
  expect(wire.joinParams().peer_id).toBe(result.current.peerId)
  expect(wire.joinParams().player_id).toBe(beforeRetry.player_id)
  await act(async () => wire.joinPush().reply("ok", { participant: { ...saved, life: 21 } }))
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

  // Socket.IO retries every few seconds during an outage; those attempts reuse the fresh token.
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

it("reconnects with a fresh token after the server refuses one, at most once per interval", async () => {
  await joinedRoom()
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] })
  expect(fetch).toHaveBeenCalledTimes(1)

  // Socket.IO does not retry a refused connection on its own.
  act(() => wire.socketError(true))
  await act(() => vi.advanceTimersByTimeAsync(0))
  expect(wire.connects).toBe(1)
  expect(fetch).toHaveBeenCalledTimes(2)
  expect(wire.socketParams().token).toBe("fresh-token")

  act(() => wire.socketError(true))
  await act(() => vi.advanceTimersByTimeAsync(TOKEN_REFRESH_INTERVAL_MS - 1))
  expect(wire.connects).toBe(1)
  await act(() => vi.advanceTimersByTimeAsync(1))
  expect(wire.connects).toBe(2)
  expect(fetch).toHaveBeenCalledTimes(3)
})

it("hydrates team state and routes life shortcuts to the viewer's team without changing personal counters", async () => {
  const { result } = await joinedRoom()
  const seats = [12, 19, 3, 7].map((id) => ({ ...saved, player_id: id, peer_id: `peer-${id}` }))
  act(() =>
    wire.emit(
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
  act(() => wire.emit("table_state", tableState()))
  expect(result.current.isOwner).toBe(false)

  act(() => wire.reconnect())
  await act(async () => wire.joinPush().reply("ok", { participant: saved, owner: true }))
  expect(result.current.isOwner).toBe(true)
})

it("a rematch keeps the seat connected and resets its local life, counters and table state", async () => {
  const { result } = await joinedRoom()
  const self = { ...saved, peer_id: result.current.peerId }
  act(() => wire.presence([self]))
  act(() => wire.emit("table_state", tableState({ peer_ids: [self.peer_id], seats: [self] })))
  act(() => wire.emit("table_log", { entries: [{ id: 9, at: 1, text: "Cody: 25 → 23" }] }))
  let rematched: Promise<boolean> = Promise.resolve(false)
  act(() => {
    rematched = result.current.rematch()
  })
  const push = wire.pushes.at(-1)!
  expect(push).toMatchObject({ event: "rematch", payload: {} })

  // The server broadcasts the fresh lobby, then tells this seat its reset copy, then replies.
  const fresh = { ...self, life: 40, poison: 0, rad: 0, commander_casts: {}, commander_damage: {} }
  act(() => {
    wire.emit(
      "table_state",
      tableState({
        timer: { started_at: null, paused_at: null, paused_ms: 0, server_now: 2000 },
        peer_ids: [self.peer_id],
        seats: [fresh],
        monarch: { holder: null, revision: 1 },
      }),
    )
    wire.emit("table_log", { entries: [{ id: 1, at: 2, text: "Rematch" }] })
    wire.emit("seat_reset", { participant: fresh })
    push.push.reply("ok")
  })
  await expect(rematched).resolves.toBe(true)

  expect(result.current.life).toBe(40)
  expect(result.current.counters).toEqual({
    poison: 0,
    rad: 0,
    commander_casts: {},
    commander_damage: {},
  })
  expect(result.current.timer?.state.started_at).toBeNull()
  expect(result.current.events.map((event) => event.text)).toEqual(["Rematch"])
  expect(result.current.closedByOwner).toBe(false)
  act(() => result.current.changeLife(-1))
  expect(payloads("update_status").at(-1)).toEqual({ life: 39 })
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
  act(() => wire.presence([self, ...watchers]))
  expect(result.current.participants.map((seat) => seat.player_name)).toEqual(["Cody"])
  expect(result.current.spectators.map((watcher) => watcher.player_name)).toEqual(["Ada", "Wren"])

  act(() => wire.presence([self]))
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

it("joins the room with only the seat identity", async () => {
  const { result } = await joinedRoom()
  expect(wire.joinParams()).toEqual({
    room_id: "room",
    peer_id: result.current.peerId,
    player_id: 7,
    deck_id: null,
  })
})

it("shows the server's card list, overlaying only changes the server has not answered", async () => {
  const { result } = await joinedRoom()
  const theirs = boardCard("theirs", "remote", "Counterspell")
  act(() => wire.emit("table_state", tableState({ cards: [theirs] })))
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
    wire.emit("identified_cards", { entries: [theirs, entry] })
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
  act(() => wire.emit("identified_cards", { entries: [] }))
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
  act(() => wire.emit("table_state", tableState({ cards: [older, newer] })))
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
    wire.emit("identified_cards", {
      entries: [older, newer, named, sameOnAnotherBoard],
    }),
  )
  await answer("named")
  await waitFor(() => expect(preloaded.slice(4)).toEqual(["/small/named", "/normal/named"]))

  // A later snapshot with nothing new fetches nothing.
  act(() => wire.emit("table_state", tableState({ cards: [older, newer, named] })))
  expect(requested()).toEqual(["newer", "older", "named"])
})

it("ignores card messages from peers and never sends cards to them", async () => {
  const { result } = await joinedRoom()
  const theirs = boardCard("theirs", "remote", "Counterspell")
  act(() => wire.emit("identified_cards", { entries: [theirs] }))
  const remote = { ...saved, player_id: 9, player_name: "Theo", peer_id: "zz-remote" }
  act(() => wire.presence([{ ...saved, peer_id: result.current.peerId }, remote]))
  const injected = boardCard("injected", result.current.peerId, "Black Lotus")
  act(() => {
    deliver(remote.peer_id, { type: "card_identified", entry: injected })
    deliver(remote.peer_id, { type: "cards_sync", entries: [injected] })
    deliver(remote.peer_id, { type: "card_removed", id: "theirs" })
    deliver(remote.peer_id, { type: "cards_cleared", ownerPeerId: "remote" })
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
  expect(wire.sent("peer_message")).toEqual([])
})

const theo = { ...saved, player_id: 9, player_name: "Theo", peer_id: "zz-remote" }

/** A message another seat sent this one, as the server relays it. */
function deliver(from: string, message: unknown) {
  wire.emit("peer_message", { from, message })
}

/** Messages this seat sent to other seats through the server. */
function sentMessages() {
  return wire
    .sent("peer_message")
    .map(
      ({ payload }) =>
        payload as { to: string; message: { type: string } & Record<string, unknown> },
    )
}

/** Lets queued negotiation steps and sender updates run to completion. */
async function flush() {
  await act(async () => new Promise((resolve) => setTimeout(resolve, 0)))
}

/** Seats Cody (this hook) and Theo, with the server having answered Cody's offer. */
async function roomWithTheo() {
  const view = await joinedRoom()
  const self = { ...saved, peer_id: view.result.current.peerId }
  act(() => wire.presence([self, theo]))
  const connection = FakePeerConnection.instances[0]!
  await waitFor(() => expect(wire.sent("sfu_offer")).toHaveLength(1))
  act(() => wire.sent("sfu_offer")[0]!.push.reply("ok", { sdp: "answer" }))
  await flush()
  return { ...view, self, connection }
}

/** The server offers Theo's board on mid "1" and its packets arrive as `stream`. */
async function serveTheoBoard(
  connection: FakePeerConnection,
  stream: MediaStream,
  owner = theo.peer_id,
) {
  act(() => wire.emit("sfu_offer", { sdp: "server-offer", tracks: { "1": owner } }))
  await waitFor(() => expect(wire.sent("sfu_answer").length).toBeGreaterThan(0))
  act(() => connection.arrive("1", stream))
}

it("publishes the camera once as three H.264-first simulcast layers and answers the server's offer", async () => {
  const { connection } = await roomWithTheo()
  expect(FakePeerConnection.instances).toHaveLength(1)
  expect(connection.addTransceiver).toHaveBeenCalledOnce()
  const [track, init] = connection.addTransceiver.mock.calls[0]!
  expect(track).not.toBeNull()
  expect(init?.direction).toBe("sendonly")
  // Lowest layer first, each one half the rows and a quarter of the bitrate of the next.
  expect(init?.sendEncodings).toEqual([
    { rid: "l", scaleResolutionDownBy: 4, maxBitrate: 156_250, maxFramerate: 30 },
    { rid: "m", scaleResolutionDownBy: 2, maxBitrate: 625_000, maxFramerate: 30 },
    { rid: "h", scaleResolutionDownBy: 1, maxBitrate: 2_500_000, maxFramerate: 30 },
  ])
  const [transceiver] = connection.transceivers
  expect(transceiver!.setCodecPreferences.mock.invocationCallOrder[0]).toBeLessThan(
    connection.createOffer.mock.invocationCallOrder[0]!,
  )
  expect(transceiver!.codecPreferences!.map((codec) => codec.mimeType)).toEqual([
    "video/H264",
    "video/H264",
    "video/VP8",
    "video/rtx",
    "video/VP9",
    "video/red",
    "video/ulpfec",
  ])
  expect(wire.sent("sfu_offer")[0]!.payload).toEqual({ sdp: "o" })
  expect(connection.setRemoteDescription).toHaveBeenCalledWith({ type: "answer", sdp: "answer" })
})

it("leaves the sender alone when presence changes nothing it encodes, and lets the server enforce reveals", async () => {
  // Chrome restarts the encoder on every setParameters and replaceTrack; a life tap at the
  // table is a presence sync, and the old mesh re-attached senders on each of them.
  const { result, self, connection } = await roomWithTheo()
  const sender = connection.sender!
  // The real camera replaced the placeholder once; nothing after that should touch it.
  expect(sender.replaceTrack).toHaveBeenCalledOnce()
  expect(sender.setParameters).not.toHaveBeenCalled()
  sender.replaceTrack.mockClear()
  for (const life of [39, 38, 37]) act(() => wire.presence([self, { ...theo, life }]))
  await flush()
  expect(sender.replaceTrack).not.toHaveBeenCalled()
  expect(sender.setParameters).not.toHaveBeenCalled()

  // A third seat drops the frame rate on every layer, once.
  const lee = { ...saved, player_id: 11, player_name: "Lee", peer_id: "zzz-lee" }
  act(() => wire.presence([self, theo, lee]))
  await flush()
  expect(sender.setParameters).toHaveBeenCalledOnce()
  expect(sender.getParameters().encodings.map((encoding) => encoding.maxFramerate)).toEqual([
    15, 15, 15,
  ])

  // A private reveal is a server rule now: the camera keeps publishing every layer.
  wire.onPush = (event, _payload, push) => {
    if (event === "reveal") push.reply("ok")
  }
  await act(() => result.current.changeReveal(lee.peer_id))
  expect(wire.sent("reveal").at(-1)!.payload).toEqual({ target: lee.peer_id })
  expect(sender.replaceTrack).not.toHaveBeenCalled()
  expect(result.current.revealTo).toBe(lee.peer_id)
})

it("spectators negotiate a receive-only connection and never publish", async () => {
  await joinedRoom({ ...saved, spectator: true })
  await waitFor(() => expect(wire.sent("sfu_offer")).toHaveLength(1))
  const connection = FakePeerConnection.instances[0]!
  expect(connection.addTransceiver).toHaveBeenCalledExactlyOnceWith("video", {
    direction: "recvonly",
  })
  expect(connection.sender).toBeUndefined()
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

it("drops malformed or unexpected peer messages without throwing", async () => {
  const { result } = await roomWithTheo()
  act(() => {
    deliver(theo.peer_id, "{not json")
    deliver(theo.peer_id, 42)
    deliver(theo.peer_id, { type: "capture_request", requestId: "r", x: 4, y: 0.5 })
    deliver(theo.peer_id, { ...crop, requestId: "never-requested" })
  })
  expect(sentMessages()).toEqual([])
  expect(result.current.capture).toBeNull()

  act(() => deliver(theo.peer_id, { type: "capture_request", requestId: "r", x: 0.5, y: 0.5 }))
  expect(sentMessages()).toEqual([
    {
      to: theo.peer_id,
      message: expect.objectContaining({
        type: "capture_response",
        requestId: "r",
        private: false,
      }),
    },
  ])
})

it("accepts only the requested peer's well-formed crop and restores the live status", async () => {
  const { result } = await roomWithTheo()
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5, true))
  expect(result.current.status).toBe("Requesting native camera crop…")
  const { requestId } = sentMessages()[0]!.message
  act(() => deliver(theo.peer_id, { ...crop, requestId, image: "data:text/html,hi" }))
  expect(result.current.capture).toBeNull()
  // Another seat cannot answer for Theo.
  act(() => deliver("zz-other", { ...crop, requestId }))
  expect(result.current.capture).toBeNull()
  act(() => deliver(theo.peer_id, { ...crop, requestId, extra: "dropped" }))
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
  const { result } = await roomWithTheo()
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
  expect(sentMessages()[0]).toEqual({
    to: theo.peer_id,
    message: { type: "capture_request", requestId: expect.any(String), x: 0.5, y: 0.5 },
  })
  const { requestId } = sentMessages()[0]!.message
  act(() => deliver(theo.peer_id, { ...crop, requestId, clickX: 100, clickY: 200 }))
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
  const { result } = await roomWithTheo()
  vi.useFakeTimers()
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5))
  const { requestId } = sentMessages()[0]!.message
  act(() => {
    vi.advanceTimersByTime(CAPTURE_TIMEOUT_MS)
  })
  expect(result.current.status).toBe("Theo's camera did not send a crop; click the card again")
  act(() => deliver(theo.peer_id, { ...crop, requestId }))
  expect(result.current.capture).toBeNull()
})

/** A `<video>` on this page decoding Theo's board at `height` rows (16:9). */
function playingTile(height: number) {
  const video = document.createElement("video")
  Object.defineProperties(video, {
    videoWidth: { value: (height * 16) / 9 },
    videoHeight: { value: height },
    readyState: { value: HTMLMediaElement.HAVE_CURRENT_DATA },
  })
  return video
}

it("crops a remote board from its own frame when that frame is the owner's native picture", async () => {
  const { result, self, connection } = await roomWithTheo()
  // Theo shows his hand to Cody alone and has opted out of training uploads.
  const owner = {
    ...theo,
    camera_height: 1080,
    shares_corrections: false,
    reveal_to: self.peer_id,
  }
  act(() => wire.presence([self, owner]))
  const stream = { id: theo.peer_id } as unknown as MediaStream
  await serveTheoBoard(connection, stream)
  const drawImage = vi.fn()
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage,
  } as unknown as CanvasRenderingContext2D)
  const stage = playingTile(1080)
  const unwatch = result.current.watchTile(stream, stage)

  act(() => result.current.requestCapture(theo.peer_id, 0.25, 0.5, true))
  // No round trip through the owner, and no waiting status.
  expect(sentMessages()).toEqual([])
  expect(result.current.status).toMatch(/^Live/)
  expect(drawImage).toHaveBeenCalledWith(stage, 160, 220, 640, 640, 0, 0, 640, 640)
  // The owner's consent and privacy travel with their seat, not the clicker's settings.
  expect(result.current.capture).toEqual({
    peerId: theo.peer_id,
    playerId: 9,
    inspect: true,
    image: "data:image/jpeg;base64,/9j/4AAQ",
    nativeWidth: 1920,
    nativeHeight: 1080,
    cropSize: 640,
    clickX: 320,
    clickY: 320,
    private: true,
    shareCorrections: false,
  })
  unwatch()
})

it("asks the owner for a crop while its board arrives below the camera's resolution", async () => {
  const { result, self, connection } = await roomWithTheo()
  act(() => wire.presence([self, { ...theo, camera_height: 1080 }]))
  const stream = { id: theo.peer_id } as unknown as MediaStream
  await serveTheoBoard(connection, stream)
  // A rail tile and a grid cell both decode a lower simulcast layer.
  result.current.watchTile(stream, playingTile(540))
  act(() => result.current.requestCapture(theo.peer_id, 0.5, 0.5))
  expect(sentMessages()).toEqual([
    {
      to: theo.peer_id,
      message: { type: "capture_request", requestId: expect.any(String), x: 0.5, y: 0.5 },
    },
  ])
  expect(result.current.status).toBe("Requesting native camera crop…")
  expect(result.current.capture).toBeNull()
})

it("maps each arriving board to its owner by mid and drops it when the server withdraws it", async () => {
  const { result, connection } = await roomWithTheo()
  // The server names the board owner per mid; the stream id is only a fallback.
  const stream = { id: "msid-from-server" } as unknown as MediaStream
  await serveTheoBoard(connection, stream)
  expect(result.current.streams).toEqual({ [theo.peer_id]: stream })
  expect(connection.setRemoteDescription).toHaveBeenLastCalledWith({
    type: "offer",
    sdp: "server-offer",
  })
  expect(wire.sent("sfu_answer")[0]!.payload).toEqual({ sdp: "a" })

  act(() => wire.emit("sfu_offer", { sdp: "server-offer-2", tracks: {} }))
  await waitFor(() => expect(wire.sent("sfu_answer")).toHaveLength(2))
  expect(result.current.streams).toEqual({})
})

it("drops a departed peer's stream so a rejoin under a new peer ID does not count twice", async () => {
  const { result, self, connection } = await roomWithTheo()
  const stream = { id: theo.peer_id } as unknown as MediaStream
  await serveTheoBoard(connection, stream)
  expect(result.current.streams).toEqual({ [theo.peer_id]: stream })
  expect(result.current.connectionStates).toEqual({ [theo.peer_id]: "new" })

  // Theo's channel drops; he rejoins with a new media generation. Presence says so first,
  // then the server offers his new board.
  const rejoined = { ...theo, peer_id: "zz-remote-2" }
  act(() => wire.presence([self, rejoined]))
  expect(result.current.streams).toEqual({})
  expect(result.current.connectionStates).toEqual({ [rejoined.peer_id]: "new" })

  const next = { id: rejoined.peer_id } as unknown as MediaStream
  await serveTheoBoard(connection, next, rejoined.peer_id)
  expect(result.current.streams).toEqual({ [rejoined.peer_id]: next })
  act(() => connection.changeState("connected"))
  expect(result.current.connectionStates).toEqual({ [rejoined.peer_id]: "connected" })
})

it("asks the server for the layer that fits the largest tile drawing each board", async () => {
  const { result, connection } = await roomWithTheo()
  const stream = { id: theo.peer_id } as unknown as MediaStream
  await serveTheoBoard(connection, stream)
  const observer = () => FakeResizeObserver.instances.at(-1)!
  const rail = document.createElement("video")
  const stage = document.createElement("video")
  const unwatchRail = result.current.watchTile(stream, rail)
  expect(observer().observed.has(rail)).toBe(true)

  // A 16:9 picture in a 400×400 rail tile is 225 rows: the quarter-resolution layer.
  act(() => observer().resize(rail, 400, 400))
  expect(payloads("sfu_layer")).toEqual([{ peer_id: theo.peer_id, layer: "l" }])
  // The pinned board is the full layer; the same size again asks for nothing new.
  const unwatchStage = result.current.watchTile(stream, stage)
  act(() => observer().resize(stage, 1600, 900))
  act(() => observer().resize(stage, 1600, 900))
  expect(payloads("sfu_layer")).toEqual([
    { peer_id: theo.peer_id, layer: "l" },
    { peer_id: theo.peer_id, layer: "h" },
  ])
  // The board also stays in the rail; shrinking the stage to a grid cell wants the middle
  // layer, and unpinning it altogether falls back to the rail's size.
  act(() => observer().resize(stage, 800, 540))
  expect(payloads("sfu_layer").at(-1)).toEqual({ peer_id: theo.peer_id, layer: "m" })
  act(() => unwatchStage())
  expect(payloads("sfu_layer").at(-1)).toEqual({ peer_id: theo.peer_id, layer: "l" })
  unwatchRail()
  // A tile showing a stream the server has not announced asks for nothing.
  const own = document.createElement("video")
  result.current.watchTile({ id: "local" } as unknown as MediaStream, own)
  act(() => observer().resize(own, 1600, 900))
  expect(payloads("sfu_layer")).toHaveLength(4)
})

it("reconnects with a fresh offer after a channel retry and abandons the old connection's steps", async () => {
  let resolveOffer!: (offer: RTCSessionDescriptionInit) => void
  const { result } = await joinedRoom()
  const first = FakePeerConnection.instances[0]!
  await waitFor(() => expect(wire.sent("sfu_offer")).toHaveLength(1))
  // A server offer still in flight when the channel drops.
  first.setRemoteDescription.mockReturnValueOnce(
    new Promise((resolve) => {
      resolveOffer = resolve as never
    }),
  )
  act(() => wire.sent("sfu_offer")[0]!.push.reply("ok", { sdp: "answer" }))
  act(() => wire.reconnect())
  expect(first.connectionState).toBe("closed")
  expect(result.current.streams).toEqual({})
  await act(async () => {
    resolveOffer({ type: "answer", sdp: "late" })
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  expect(first.addIceCandidate).not.toHaveBeenCalled()

  await act(async () => wire.joinPush().reply("ok", { participant: saved }))
  await waitFor(() => expect(FakePeerConnection.instances).toHaveLength(2))
  await waitFor(() => expect(wire.sent("sfu_offer")).toHaveLength(2))
  expect(result.current.error).toBeNull()
})

it("reports a rejected offer and logs later failed steps instead of leaving unhandled rejections", async () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {})
  const { result } = await joinedRoom()
  await waitFor(() => expect(wire.sent("sfu_offer")).toHaveLength(1))
  act(() => wire.sent("sfu_offer")[0]!.push.reply("error", { reason: "offer rejected" }))
  await waitFor(() =>
    expect(result.current.error).toBe("Could not connect to the table's video server."),
  )
  expect(warn).toHaveBeenCalledWith(
    "WebRTC negotiation with the table server failed",
    expect.anything(),
  )

  const connection = FakePeerConnection.instances[0]!
  connection.setRemoteDescription.mockRejectedValueOnce(
    new DOMException("bad sdp", "InvalidStateError"),
  )
  act(() => {
    wire.emit("sfu_offer", { sdp: "bad", tracks: {} })
    wire.emit("sfu_candidate", { candidate: { candidate: "c" } })
  })
  await waitFor(() => expect(warn).toHaveBeenCalledTimes(2))
  expect(connection.createAnswer).not.toHaveBeenCalled()
  expect(wire.sent("sfu_answer")).toEqual([])
  // The candidate arrived before any description, so it waits instead of failing.
  expect(connection.addIceCandidate).not.toHaveBeenCalled()
})
