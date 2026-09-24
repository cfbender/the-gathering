import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { EMPTY_TURNS } from "./turns"
import { wire } from "./test-support/fake-phoenix"
import {
  FakePeerConnection,
  fakeMedia,
  installFakeMedia,
  installFakeWebRtc,
  serveTableConfig,
} from "./test-support/fake-webrtc"
import { useWebcamRoom, type BoardCard, type TableParticipant } from "./use-webcam-room"

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
  expect(wire.sent("cards").at(-1)?.payload).toEqual({ type: "card_identified", entry })
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
