import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { StreamVideo } from "./board"
import type { TableParticipant } from "./room-types"
import { WebcamTablePage } from "./webcam-table-page"

const mode = vi.hoisted(() => ({
  started: false,
  spectator: false,
  spectators: [] as TableParticipant[],
}))
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
        // "self" stands in for this tab's peer ID, which is random per render.
        spectators: mode.spectators.map((watcher) =>
          watcher.peer_id === "self" ? { ...watcher, peer_id: room.peerId } : watcher,
        ),
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
  mode.spectators = []
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
  expect(screen.getByRole("menuitem", { name: "Reveal hand…" })).toBeTruthy()
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

it("lists spectators in the Table tab for seated players and for the spectators themselves", () => {
  renderTable(true)
  expect(screen.queryByRole("list", { name: "Spectators" })).toBeNull()
  cleanup()

  const spectator = (
    peer_id: string,
    player_id: number,
    player_name: string,
  ): TableParticipant => ({
    peer_id,
    player_id,
    player_name,
    joined_at: 200,
    life: 40,
    poison: 0,
    rad: 0,
    commander_casts: {},
    commander_damage: {},
    camera_off: true,
    eliminated: false,
    spectator: true,
  })
  mode.spectators = [spectator("watcher", 3, "Wren")]
  renderTable(true)
  expect(screen.getByTitle("Spectators").textContent).toMatch(/1\s*spectating/)
  let list = screen.getByRole("list", { name: "Spectators" })
  expect(within(list).getByText("Wren")).toBeTruthy()
  expect(within(list).queryByText("(you)")).toBeNull()
  cleanup()

  // A spectator sees themselves marked in the list; their fallback seat carries their peer ID.
  mode.spectators = [spectator("self", 1, "Cody"), spectator("watcher", 3, "Wren")]
  renderTable(true, true)
  list = screen.getByRole("list", { name: "Spectators" })
  expect(within(list).getAllByRole("listitem")).toHaveLength(2)
  expect(within(list).getByText("(you)")).toBeTruthy()
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
