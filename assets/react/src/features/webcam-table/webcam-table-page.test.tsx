import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { useMemo } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import type { GalleryArt } from "./recognition/pipeline"
import type { CapturedCard } from "./use-webcam-room"
import { WebcamTablePage } from "./webcam-table-page"

// Keep the real room hook (including announceCard/deduplication); replace only camera capture
// and the unavailable recognition worker with an explicit two-printing picker fixture.
vi.mock("./use-webcam-room", async (importOriginal) => {
  const original = await importOriginal<typeof import("./use-webcam-room")>()
  return {
    ...original,
    useWebcamRoom: (...args: Parameters<typeof original.useWebcamRoom>) => {
      const room = original.useWebcamRoom(...args)
      const capture = useMemo<CapturedCard>(
        () => ({
          peerId: room.peerId,
          playerId: 1,
          image: "fixture",
          nativeWidth: 640,
          nativeHeight: 640,
          cropSize: 640,
          clickX: 320,
          clickY: 320,
          inspect: true,
          private: false,
        }),
        [room.peerId],
      )
      return { ...room, capture }
    },
  }
})
vi.mock("./recognition/use-recognizer", () => {
  const identify = () => Promise.reject(new Error("not installed"))
  return {
    decodeImage: async () => ({ data: new Uint8ClampedArray(4), width: 1, height: 1 }),
    useRecognizer: () => ({ ready: false, state: { status: "unavailable" }, identify }),
  }
})
vi.mock("./side-panel", () => ({ SidePanel: () => null }))
vi.mock("./card-suggestions", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./card-suggestions")>()),
  CardSuggestions: ({ onChooseCard }: { onChooseCard: (art: GalleryArt) => void }) => (
    <div>
      <button
        onClick={() => onChooseCard({ id: "first", name: "Forest", set: "lea", frame: "1993" })}
      >
        Identify first Forest
      </button>
      <button
        onClick={() => onChooseCard({ id: "second", name: "Forest", set: "fin", frame: "2015" })}
      >
        Identify second Forest
      </button>
    </div>
  ),
}))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

it("previews the newly identified printing while retaining one original tray entry and its actions", async () => {
  // No socket or media is needed for the local identification path.
  vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("{}", { status: 503 }))
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  client.setQueryData(["session"], { id: 7, role: "member" })
  client.setQueryData(["players"], [{ id: 1, user_id: 7, name: "Cody" }])
  client.setQueryData(["decks", {}], [])
  client.setQueryData(["card-printings", "preview", "Forest"], [])
  for (const [id, set] of [
    ["first", "lea"],
    ["second", "fin"],
  ]) {
    client.setQueryData(["card-printings", id, "details"], {
      name: "Forest",
      image_uris: {},
      set_code: set,
      collector_number: id === "first" ? "280" : "300",
      power: null,
      toughness: null,
      loyalty: null,
      prices: { usd: null, usd_foil: null, usd_etched: null },
    })
  }
  client.setQueryData(["card-printings", "second", "rulings"], [])
  render(
    <QueryClientProvider client={client}>
      <WebcamTablePage roomId="preview-test" />
    </QueryClientProvider>,
  )
  fireEvent.click(await screen.findByRole("button", { name: "Identify first Forest" }))
  expect(screen.getByText(/LEA · #280/)).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Close card details" }))
  fireEvent.click(screen.getByRole("button", { name: "Identify second Forest" }))
  expect(screen.getByText(/FIN · #300/)).toBeTruthy()
  const tray = screen.getByRole("region", { name: "Cards identified on Cody's board" })
  fireEvent.click(within(tray).getByRole("button"))
  expect(within(tray).getAllByRole("listitem")).toHaveLength(1)
  fireEvent.click(screen.getByRole("button", { name: "Close card details" }))
  fireEvent.click(within(tray).getByRole("button", { name: "Show Forest" }))
  expect(screen.getByText(/LEA · #280/)).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Close card details" }))
  fireEvent.click(screen.getByRole("button", { name: "Identify second Forest" }))
  fireEvent.click(screen.getByRole("button", { name: "Rulings" }))
  expect(screen.getByText("No rulings published on Scryfall for this card.")).toBeTruthy()
  fireEvent.keyDown(document.activeElement!, { key: "Escape" })
  fireEvent.click(screen.getByRole("button", { name: "Wrong card?" }))
  fireEvent.click(screen.getByRole("button", { name: "Identify second Forest" }))
  expect(within(tray).getAllByRole("listitem")).toHaveLength(1)
  expect(screen.getByText(/FIN · #300/)).toBeTruthy()
  fireEvent.click(screen.getByRole("button", { name: "Remove" }))
  expect(within(tray).queryAllByRole("listitem")).toHaveLength(0)
})
