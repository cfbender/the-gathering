import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import { EMPTY_COUNTERS } from "./seat-counters"
import { WebcamTablePage } from "./webcam-table-page"

const remote = vi.hoisted(() => ({ peerId: "first-connection", requestCapture: vi.fn() }))
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

async function toggleFlip(action: "Flip" | "Unflip") {
  const rail = screen.getByRole("complementary", { name: "Player cameras" })
  const trigger = within(rail).getByRole("button", { name: "Theo's seat actions" })
  act(() => trigger.focus())
  fireEvent.keyDown(trigger, { key: "ArrowDown" })
  fireEvent.click(
    await screen.findByRole("menuitem", { name: `${action} Theo's video vertically` }),
  )
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
