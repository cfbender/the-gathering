import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { useWebcamRoom, type TableParticipant } from "./use-webcam-room"
import { EMPTY_TURNS } from "./turns"

const wire = vi.hoisted(() => ({
  joined: null as null | ((reply: { participant: TableParticipant }) => void),
  events: new Map<string, (data: unknown) => void>(),
  error: () => {},
  channelError: () => {},
  params: () => ({ token: "" }),
  channelParams: () => ({ peer_id: "", player_id: 0 }),
  push: vi.fn((_event: string, _payload: unknown) => ({ receive: vi.fn() })),
  camera: vi.fn(),
}))
vi.mock("phoenix", () => ({
  Socket: class {
    constructor(_url: string, options: { params: typeof wire.params }) {
      wire.params = options.params
    }
    connect() {}
    disconnect() {}
    onError(callback: () => void) {
      wire.error = callback
    }
    channel(_topic: string, params: typeof wire.channelParams) {
      wire.channelParams = params
      return {
        state: "joined",
        on: (event: string, callback: (data: unknown) => void) => wire.events.set(event, callback),
        onError(callback: () => void) {
          wire.channelError = callback
        },
        leave() {},
        push: wire.push,
        join() {
          const reply = {
            receive: (event: string, callback: typeof wire.joined) => {
              if (event === "ok") wire.joined = callback
              return reply
            },
          }
          return reply
        },
      }
    }
  },
  Presence: class {
    onJoin() {}
    onLeave() {}
    onSync() {}
  },
}))
vi.mock("./camera", () => ({ openCamera: () => wire.camera() }))

beforeEach(() => {
  wire.joined = null
  wire.events.clear()
  wire.push.mockClear()
  const track = { enabled: true, stop: vi.fn(), getSettings: () => ({ height: 1080 }) }
  const media = { getTracks: () => [track], getVideoTracks: () => [track] }
  wire.camera.mockResolvedValue(media)
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue()
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null)
  Object.defineProperty(HTMLCanvasElement.prototype, "captureStream", {
    configurable: true,
    value: () => media,
  })
  vi.spyOn(globalThis, "fetch").mockImplementation(
    async () =>
      new Response(
        JSON.stringify({
          data: { socket_token: "fresh-token", ice_servers: [], max_players: 10 },
        }),
      ),
  )
})
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  wire.camera.mockClear()
})

function renderRoom() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return renderHook(() => useWebcamRoom("room", 7, null), {
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  })
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

it("hydrates before editing and reconnects without republishing default life or counters", async () => {
  const { result } = renderRoom()
  await waitFor(() => expect(wire.joined).not.toBeNull())
  await act(async () => wire.joined?.({ participant: saved }))
  expect(result.current.life).toBe(23)
  expect(result.current.counters.commander_damage).toEqual({ 19: { Atraxa: 11 } })
  expect(wire.push).toHaveBeenCalledWith("update_status", { camera_off: false })
  expect(wire.push).not.toHaveBeenCalledWith("update_status", expect.objectContaining({ life: 40 }))
  act(() => result.current.changeLife(-2))
  expect(wire.push).toHaveBeenLastCalledWith("update_status", { life: 21 })
  act(() => wire.error())
  expect(result.current.status).toMatch(/Reconnecting/)
  const beforeRetry = wire.channelParams()
  act(() => wire.channelError())
  expect(wire.channelParams().peer_id).not.toBe(beforeRetry.peer_id)
  expect(wire.channelParams().peer_id).toBe(result.current.peerId)
  expect(wire.channelParams().player_id).toBe(beforeRetry.player_id)
  await act(async () => wire.joined?.({ participant: { ...saved, life: 21 } }))
  expect(result.current.life).toBe(21)
  expect(result.current.error).toBeNull()
  expect(wire.camera).toHaveBeenCalledOnce()
  await waitFor(() => expect(fetch).toHaveBeenCalledTimes(2))
  expect(wire.params().token).toBe("fresh-token")
})

it("hydrates team state and routes life shortcuts to the viewer's team without changing personal counters", async () => {
  const { result } = renderRoom()
  await waitFor(() => expect(wire.joined).not.toBeNull())
  await act(async () => wire.joined?.({ participant: saved }))
  const seats = [12, 19, 3, 7].map((id) => ({ ...saved, player_id: id, peer_id: `peer-${id}` }))
  act(() =>
    wire.events.get("table_state")?.({
      timer: { started_at: 1000, paused_at: null, paused_ms: 0, server_now: 1000 },
      peer_ids: seats.map((seat) => seat.peer_id),
      seats,
      eliminated_seats: [],
      turns: EMPTY_TURNS,
      auto_randomize: false,
      owner_id: 12,
      mode: "two_headed_giant",
      team_life: { 0: 56, 1: 61 },
    }),
  )
  expect(result.current.mode).toBe("two_headed_giant")
  expect(result.current.teamLife).toEqual({ 0: 56, 1: 61 })
  act(() => result.current.changeLife(-10))
  expect(wire.push).toHaveBeenLastCalledWith("adjust_team_life", { team_index: 1, delta: -10 })
  expect(result.current.life).toBe(23)
  expect(result.current.counters.poison).toBe(6)
})

it("late spectators never request a camera or publish life/counters", async () => {
  const { result } = renderRoom()
  await waitFor(() => expect(wire.joined).not.toBeNull())
  await act(async () => wire.joined?.({ participant: { ...saved, spectator: true } }))
  expect(result.current.spectating).toBe(true)
  expect(result.current.status).toMatch(/Spectating/)
  expect(wire.camera).not.toHaveBeenCalled()
  act(() => {
    result.current.changeLife(-1)
    result.current.adjustCounter({ kind: "poison" }, 1)
  })
  expect(wire.push.mock.calls.filter(([event]) => event === "update_status")).toEqual([])
})

it("keeps the hidden capture video playing after the camera replaces the placeholder", async () => {
  // Swapping srcObject pauses a media element; a paused capture video would hand every
  // click the same frozen first frame instead of what is on the table now.
  const track = { enabled: true, stop: vi.fn(), getSettings: () => ({ height: 1080 }) }
  const camera = { getTracks: () => [track], getVideoTracks: () => [track] }
  wire.camera.mockResolvedValue(camera)
  const playedSources: unknown[] = []
  vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(function (
    this: HTMLVideoElement,
  ) {
    playedSources.push(this.srcObject)
    return Promise.resolve()
  })
  renderRoom()
  await waitFor(() => expect(wire.joined).not.toBeNull())
  await act(async () => wire.joined?.({ participant: saved }))
  await waitFor(() => expect(wire.camera).toHaveBeenCalledOnce())
  await waitFor(() => expect(playedSources).toContain(camera))
})
