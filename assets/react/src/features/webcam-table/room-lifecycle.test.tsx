import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { StreamVideo } from "./board"
import { WebcamTablePage } from "./webcam-table-page"

const mode = vi.hoisted(() => ({ started: false, spectator: false }))
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: ReactNode }) => <a>{children}</a>,
}))
vi.mock("./use-webcam-room", async (importOriginal) => {
  const original = await importOriginal<typeof import("./use-webcam-room")>()
  return {
    ...original,
    useWebcamRoom: (...args: Parameters<typeof original.useWebcamRoom>) => {
      const room = original.useWebcamRoom(...args)
      return {
        ...room,
        spectating: mode.spectator,
        isOwner: !mode.spectator,
        participants: [
          {
            peer_id: mode.spectator ? "other" : room.peerId,
            player_id: mode.spectator ? 2 : 1,
            player_name: mode.spectator ? "Theo" : "Cody",
            joined_at: 100,
            life: 23,
            poison: 0,
            rad: 0,
            commander_casts: {},
            commander_damage: {},
            camera_off: false,
            eliminated: false,
          },
        ],
        timer: {
          state: {
            started_at: mode.started ? 100 : null,
            paused_at: null,
            paused_ms: 0,
            server_now: 300,
          },
          receivedAt: performance.now(),
        },
      }
    },
  }
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function renderTable(started: boolean, spectator = false) {
  mode.started = started
  mode.spectator = spectator
  vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}", { status: 503 }))
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  client.setQueryData(["session"], { id: 7, role: "member" })
  client.setQueryData(["players"], [{ id: 1, user_id: 7, name: "Cody" }])
  client.setQueryData(["decks", {}], [])
  return render(
    <QueryClientProvider client={client}>
      <WebcamTablePage roomId="lifecycle" />
    </QueryClientProvider>,
  )
}

it("shows open seats before start and none after the shared timer starts", () => {
  renderTable(false)
  expect(screen.getAllByText("Open seat")).toHaveLength(9)
  cleanup()
  renderTable(true)
  expect(screen.queryByText("Open seat")).toBeNull()
})

it("offers Eliminate player in the seat menu only after the match starts", async () => {
  const openMenu = async () => {
    const trigger = screen.getAllByRole("button", { name: "Cody's seat actions" })[0]!
    act(() => trigger.focus())
    fireEvent.keyDown(trigger, { key: "ArrowDown" })
    await screen.findByRole("menu")
  }
  renderTable(false)
  expect(screen.queryByRole("button", { name: /Eliminat/ })).toBeNull()
  await openMenu()
  expect(screen.getByRole("menuitem", { name: "Pin as active board" })).toBeTruthy()
  expect(screen.queryByRole("menuitem", { name: /Eliminate|Restore/ })).toBeNull()
  cleanup()
  renderTable(true)
  expect(screen.queryByRole("button", { name: /Eliminat/ })).toBeNull()
  await openMenu()
  expect(screen.getByRole("menuitem", { name: "Eliminate player" })).toBeTruthy()
})

it("renders a late joiner as a spectator without a phantom seat or game controls", () => {
  renderTable(true, true)
  expect(screen.getByText(/Spectating — this game/)).toBeTruthy()
  const cameras = screen.getByRole("complementary", { name: "Player cameras" })
  expect(within(cameras).getByRole("button", { name: "Show Theo's board" })).toBeTruthy()
  expect(within(cameras).queryByText("Cody")).toBeNull()
  expect(screen.queryByText("Open seat")).toBeNull()
  expect(
    screen.queryByRole("button", {
      name: /Pass turn|End game|Turn camera off|Increase life|Decrease life/,
    }),
  ).toBeNull()
  expect(screen.queryByRole("button", { name: /Add a turn|Remove a turn|Eliminated:/ })).toBeNull()
  expect(screen.queryByRole("button", { name: "Decks" })).toBeNull()
})

it("allows video-only spectator autoplay without camera permission or a click", () => {
  const stream = {} as MediaStream
  const { container } = render(<StreamVideo stream={stream} />)
  const video = container.querySelector("video")!
  expect(video.srcObject).toBe(stream)
  expect(video.autoplay).toBe(true)
  expect(video.muted).toBe(true)
  expect(video.playsInline).toBe(true)
})
