import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { EMPTY_COUNTERS } from "./seat-counters"
import { WebcamTablePage } from "./webcam-table-page"

const remote = vi.hoisted(() => ({
  peerId: "first-connection",
  requestCapture: vi.fn(),
  activePlayerId: null as number | null,
}))
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: ReactNode }) => <a>{children}</a>,
}))
vi.mock("./use-webcam-room", async (importOriginal) => {
  const original = await importOriginal<typeof import("./use-webcam-room")>()
  const stream = {} as MediaStream
  return {
    ...original,
    useWebcamRoom: (...args: Parameters<typeof original.useWebcamRoom>) => {
      const room = original.useWebcamRoom(...args)
      return {
        ...room,
        requestCapture: remote.requestCapture,
        turns: { ...room.turns, active_player_id: remote.activePlayerId },
        localStream: stream,
        streams: { [remote.peerId]: stream, "other-player": stream },
        participants: [
          { peer_id: remote.peerId, player_id: 42, player_name: "Theo" },
          { peer_id: "other-player", player_id: 73, player_name: "Mara" },
        ].map((player) => ({
          ...EMPTY_COUNTERS,
          ...player,
          joined_at: 100,
          life: 40,
          camera_off: false,
          eliminated: false,
        })),
      }
    },
  }
})

beforeEach(() => {
  localStorage.clear()
  remote.peerId = "first-connection"
  remote.activePlayerId = null
  remote.requestCapture.mockClear()
  vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}", { status: 503 }))
})
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function renderTable(roomId = "first-room", viewerId = 1) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  client.setQueryData(["session"], { id: 7, role: "member" })
  client.setQueryData(["players"], [{ id: viewerId, user_id: 7, name: "Cody" }])
  client.setQueryData(["decks", {}], [])
  return render(
    <QueryClientProvider client={client}>
      <WebcamTablePage roomId={roomId} />
    </QueryClientProvider>,
  )
}

function tileVideo(name: string) {
  return screen.getByRole("button", { name: `Show ${name}'s board` }).querySelector("video")!
}

async function toggleFlip(action: "Flip" | "Unflip", axis: "vertical" | "horizontal" = "vertical") {
  const rail = screen.getByRole("complementary", { name: "Player cameras" })
  const trigger = within(rail).getByRole("button", { name: "Theo's seat actions" })
  act(() => trigger.focus())
  fireEvent.keyDown(trigger, { key: "ArrowDown" })
  fireEvent.click(await screen.findByRole("menuitem", { name: `${action} Theo's video ${axis}ly` }))
}

it("persists a remote player's flip across reloads, changed peer IDs and rooms, then clears it", async () => {
  renderTable()
  await toggleFlip("Flip")
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(true)
  expect(tileVideo("Mara").classList.contains("-scale-y-100")).toBe(false)
  expect(tileVideo("Cody").classList.contains("-scale-y-100")).toBe(false)
  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  expect(
    screen
      .getByRole("button", { name: "Inspect Theo's board" })
      .querySelector("video")!
      .classList.contains("-scale-y-100"),
  ).toBe(true)
  expect(JSON.parse(localStorage.getItem("the-gathering:table-preferences:1")!)).toMatchObject({
    flippedPlayerIds: [42],
  })

  cleanup()
  remote.peerId = "reconnected-in-another-room"
  renderTable("second-room")
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(true)
  expect(tileVideo("Mara").classList.contains("-scale-y-100")).toBe(false)
  await toggleFlip("Unflip")
  cleanup()
  renderTable("third-room")
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(false)
  expect(JSON.parse(localStorage.getItem("the-gathering:table-preferences:1")!)).toMatchObject({
    flippedPlayerIds: [],
  })
})

it("keeps another viewer's preferences and the local preview unchanged", async () => {
  renderTable()
  await toggleFlip("Flip")
  cleanup()
  renderTable("first-room", 9)
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(false)
  const rail = screen.getByRole("complementary", { name: "Player cameras" })
  const trigger = within(rail).getByRole("button", { name: "Cody's seat actions" })
  act(() => trigger.focus())
  fireEvent.keyDown(trigger, { key: "ArrowDown" })
  await screen.findByRole("menuitem", { name: "Turn camera off" })
  expect(screen.queryByRole("menuitem", { name: /Flip Cody/ })).toBeNull()
})

it("maps inspection clicks back to the unflipped source, including letterbox edges", async () => {
  renderTable()
  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  const board = screen.getByRole("button", { name: "Inspect Theo's board" })
  const video = board.querySelector("video")!
  Object.defineProperties(video, { videoWidth: { value: 800 }, videoHeight: { value: 400 } })
  vi.spyOn(board, "getBoundingClientRect").mockReturnValue(new DOMRect(20, 30, 400, 400))
  // Video occupies x=20..420, y=130..330. Off-center point is (0.2, 0.25).
  fireEvent.click(board, { clientX: 100, clientY: 180 })
  expect(remote.requestCapture).toHaveBeenLastCalledWith("first-connection", 0.2, 0.25, false)
  await toggleFlip("Flip")
  fireEvent.click(board, { clientX: 100, clientY: 180, shiftKey: true })
  expect(remote.requestCapture).toHaveBeenLastCalledWith("first-connection", 0.2, 0.75, true)
  fireEvent.click(board, { clientX: 100, clientY: 40 })
  expect(remote.requestCapture).toHaveBeenLastCalledWith("first-connection", 0.2, 1, false)
  fireEvent.click(board, { clientX: 100, clientY: 420 })
  expect(remote.requestCapture).toHaveBeenLastCalledWith("first-connection", 0.2, 0, false)
})

it("flips horizontally on its own or combined with a vertical flip, and remembers both", async () => {
  renderTable()
  await toggleFlip("Flip", "horizontal")
  expect(tileVideo("Theo").classList.contains("-scale-x-100")).toBe(true)
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(false)
  expect(tileVideo("Mara").classList.contains("-scale-x-100")).toBe(false)
  expect(tileVideo("Cody").classList.contains("-scale-x-100")).toBe(false)
  await toggleFlip("Flip", "vertical")
  expect(tileVideo("Theo").classList.contains("-scale-x-100")).toBe(true)
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(true)
  expect(JSON.parse(localStorage.getItem("the-gathering:table-preferences:1")!)).toMatchObject({
    flippedPlayerIds: [42],
    horizontallyFlippedPlayerIds: [42],
  })

  cleanup()
  remote.peerId = "reconnected-in-another-room"
  renderTable("second-room")
  expect(tileVideo("Theo").classList.contains("-scale-x-100")).toBe(true)
  await toggleFlip("Unflip", "horizontal")
  expect(tileVideo("Theo").classList.contains("-scale-x-100")).toBe(false)
  expect(tileVideo("Theo").classList.contains("-scale-y-100")).toBe(true)
  expect(JSON.parse(localStorage.getItem("the-gathering:table-preferences:1")!)).toMatchObject({
    flippedPlayerIds: [42],
    horizontallyFlippedPlayerIds: [],
  })
})

it("maps inspection clicks on a horizontally flipped board back to the source", async () => {
  renderTable()
  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  const board = screen.getByRole("button", { name: "Inspect Theo's board" })
  const video = board.querySelector("video")!
  Object.defineProperties(video, { videoWidth: { value: 800 }, videoHeight: { value: 400 } })
  vi.spyOn(board, "getBoundingClientRect").mockReturnValue(new DOMRect(20, 30, 400, 400))
  await toggleFlip("Flip", "horizontal")
  fireEvent.click(board, { clientX: 100, clientY: 180 })
  expect(remote.requestCapture).toHaveBeenLastCalledWith("first-connection", 0.8, 0.25, false)
  await toggleFlip("Flip", "vertical")
  fireEvent.click(board, { clientX: 100, clientY: 180 })
  expect(remote.requestCapture).toHaveBeenLastCalledWith("first-connection", 0.8, 0.75, false)
})

it("soft-pins a clicked board over follow-turn until it is clicked again", () => {
  remote.activePlayerId = 73
  renderTable()
  expect(screen.getByRole("button", { name: "Inspect Mara's board" })).toBeTruthy()
  expect(screen.queryByRole("button", { name: "Follow turn" })).toBeNull()

  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  expect(screen.getByRole("button", { name: "Inspect Theo's board" })).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  expect(screen.getByRole("button", { name: "Inspect Mara's board" })).toBeTruthy()

  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  fireEvent.click(screen.getByRole("button", { name: "Follow turn" }))
  expect(screen.getByRole("button", { name: "Inspect Mara's board" })).toBeTruthy()
  expect(JSON.parse(localStorage.getItem("the-gathering:table-preferences:1")!)).toMatchObject({
    viewMode: "follow",
  })
})

it("shows every camera in grid view and fills the stage with a clicked one until clicked again", () => {
  localStorage.setItem("the-gathering:table-preferences:1", JSON.stringify({ viewMode: "grid" }))
  remote.activePlayerId = 73
  renderTable()
  const grid = screen.getByRole("region", { name: "Camera grid" })
  for (const name of ["Cody", "Theo", "Mara"])
    expect(within(grid).getByRole("button", { name: `Show ${name}'s board` })).toBeTruthy()
  expect(screen.queryByRole("complementary", { name: "Player cameras" })).toBeNull()
  expect(screen.queryByRole("button", { name: /Inspect/ })).toBeNull()

  fireEvent.click(within(grid).getByRole("button", { name: "Show Theo's board" }))
  expect(screen.getByRole("button", { name: "Inspect Theo's board" })).toBeTruthy()
  const rail = screen.getByRole("complementary", { name: "Player cameras" })
  fireEvent.click(within(rail).getByRole("button", { name: "Show Theo's board" }))
  expect(screen.getByRole("region", { name: "Camera grid" })).toBeTruthy()

  fireEvent.click(screen.getByRole("button", { name: "Show Mara's board" }))
  fireEvent.click(screen.getByRole("button", { name: "Back to grid" }))
  expect(screen.getByRole("region", { name: "Camera grid" })).toBeTruthy()
})

it("toggles grid view with G and drops a pin made in the other view", () => {
  remote.activePlayerId = 73
  renderTable()
  fireEvent.keyDown(window, { key: "g" })
  expect(screen.getByRole("region", { name: "Camera grid" })).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Show Theo's board" }))
  expect(screen.getByRole("button", { name: "Inspect Theo's board" })).toBeTruthy()

  fireEvent.keyDown(window, { key: "g" })
  expect(screen.getByRole("button", { name: "Inspect Mara's board" })).toBeTruthy()
  expect(JSON.parse(localStorage.getItem("the-gathering:table-preferences:1")!)).toMatchObject({
    viewMode: "follow",
  })
  cleanup()
  renderTable()
  expect(screen.getByRole("button", { name: "Inspect Mara's board" })).toBeTruthy()
})
