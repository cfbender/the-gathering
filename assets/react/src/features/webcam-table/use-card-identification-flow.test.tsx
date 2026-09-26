import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { renderHook, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test"
import type { DecklistCards } from "@/features/decks/decklist-cards"
import type { Identification } from "./recognition/messages"
import type { Candidate, GalleryArt } from "./recognition/pipeline"
import type { CapturedCard, TableParticipant } from "./room-types"
import type { WebcamRoom } from "./table-view"
import { useCardIdentificationFlow } from "./use-card-identification-flow"
import type { useCorrectionUpload } from "./use-correction-upload"

const recognizer = vi.hoisted(() => ({
  identify: vi.fn(),
  locate: vi.fn(),
  search: vi.fn(),
  printings: vi.fn(),
}))

vi.mock("./recognition/use-recognizer", () => ({
  decodeImage: async () => ({ data: new Uint8ClampedArray(4), width: 1, height: 1 }),
  useRecognizer: () => ({
    ready: true,
    state: { status: "ready", version: "v1", arts: 3, loadMs: 1 },
    ...recognizer,
  }),
}))

const seat: TableParticipant = {
  peer_id: "peer-cody",
  player_id: 1,
  player_name: "Cody",
  life: 40,
  joined_at: 0,
  camera_off: false,
  eliminated: false,
  deck_id: 10,
} as TableParticipant

const capture: CapturedCard = {
  peerId: "peer-cody",
  playerId: 1,
  image: "fixture",
  nativeWidth: 640,
  nativeHeight: 640,
  cropSize: 640,
  clickX: 320,
  clickY: 320,
  inspect: false,
  private: false,
  shareCorrections: false,
} as CapturedCard

function candidate(id: string, name: string, score: number): Candidate {
  return { id, name, set: "tst", frame: "modern", index: 0, score }
}

function identification(candidates: Candidate[]): Identification {
  return {
    quad: [
      [0, 0],
      [1, 0],
      [1, 1],
      [0, 1],
    ],
    upVote: 1,
    candidates,
    timings: { detector: 1, embed: 1, search: 1, total: 3 },
  }
}

const list: DecklistCards = {
  source: "moxfield",
  url: "https://moxfield.com/decks/x",
  name: "Deck",
  fetched_at: "2026-09-26T00:00:00Z",
  cards: [
    {
      name: "Sol Ring",
      quantity: 1,
      zone: "mainboard",
      printing_id: "deck-sol-ring",
      card_id: null,
      type_line: null,
      mana_cost: null,
      cmc: null,
      game_changer: false,
      image_uris: {},
    },
  ],
}

let room: WebcamRoom
const saveCorrection = vi.fn()
const corrections = { save: saveCorrection } as unknown as ReturnType<typeof useCorrectionUpload>

beforeEach(() => {
  room = {
    capture,
    participants: [seat],
    peerId: "peer-cody",
    dismissCapture: vi.fn(),
    chooseDeck: vi.fn(),
    removeCard: vi.fn(),
    announceCard: vi.fn((ownerPeerId: string, byPlayerName: string, card: unknown) => ({
      id: "entry",
      ownerPeerId,
      byPlayerName,
      card,
      at: 1,
    })),
  } as unknown as WebcamRoom
  // The gallery art "sol-ring-art" holds the list's exact printing.
  recognizer.locate.mockResolvedValue([
    {
      id: "sol-ring-art",
      name: "Sol Ring",
      set: "c21",
      frame: "modern",
      printings: [{ id: "deck-sol-ring", name: "Sol Ring", set: "cmm", collector_number: "400" }],
    } satisfies GalleryArt,
  ])
})

afterEach(() => vi.clearAllMocks())

function renderFlow(decklists: ReadonlyMap<string, DecklistCards>) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
  const hook = renderHook(
    () =>
      useCardIdentificationFlow({
        room,
        seated: [seat],
        decks: [],
        decklists,
        playerName: "Cody",
        corrections,
        blocked: false,
      }),
    { wrapper },
  )
  return { ...hook, client }
}

it("records a near-tie deck card as the owner's listed printing", async () => {
  // 0.05 apart: not clear on its own; the deck prior lifts Sol Ring to a clear 0.08 lead.
  const raw = identification([
    candidate("sol-ring-art", "Sol Ring", 0.75),
    candidate("mind-stone", "Mind Stone", 0.7),
    candidate("arcane-signet", "Arcane Signet", 0.6),
  ])
  // The list (and its located printings) loads long before a click in play; hold the click
  // until then so the test does not race the lookup.
  let answer!: (value: Identification) => void
  recognizer.identify.mockReturnValue(new Promise((resolve) => (answer = resolve)))
  const { result, client } = renderFlow(new Map([["peer-cody", list]]))
  await waitFor(() =>
    expect(client.getQueryData(["gallery-locate", "v1", ["deck-sol-ring"]])).toBeDefined(),
  )
  answer(raw)

  await waitFor(() => expect(room.announceCard).toHaveBeenCalled())
  expect(room.announceCard).toHaveBeenCalledWith(
    "peer-cody",
    "Cody",
    expect.objectContaining({ id: "deck-sol-ring", set: "cmm", collector_number: "400" }),
  )
  // Corrections keep the recognizer's own answer and the gallery art that was chosen.
  expect(saveCorrection).toHaveBeenCalledWith(capture, raw, "sol-ring-art", "v1", false)
  expect(result.current.ownerDeckNames?.has("sol ring")).toBe(true)
})

it("leaves the same near-tie to the picker without a deck list", async () => {
  recognizer.identify.mockResolvedValue(
    identification([
      candidate("sol-ring-art", "Sol Ring", 0.75),
      candidate("mind-stone", "Mind Stone", 0.7),
    ]),
  )
  const { result } = renderFlow(new Map())

  await waitFor(() => expect(result.current.pickerOpen).toBe(true))
  expect(room.announceCard).not.toHaveBeenCalled()
  expect(recognizer.locate).not.toHaveBeenCalled()
})

it("keeps a clearly better card that is not in the list", async () => {
  recognizer.identify.mockResolvedValue(
    identification([
      candidate("mind-stone", "Mind Stone", 0.86),
      candidate("sol-ring-art", "Sol Ring", 0.74),
    ]),
  )
  renderFlow(new Map([["peer-cody", list]]))

  await waitFor(() => expect(room.announceCard).toHaveBeenCalled())
  expect(room.announceCard).toHaveBeenCalledWith(
    "peer-cody",
    "Cody",
    expect.objectContaining({ id: "mind-stone", name: "Mind Stone" }),
  )
})

it("reorders a close call so the deck card is option 1 in the picker", async () => {
  recognizer.identify.mockResolvedValue(
    identification([
      candidate("mind-stone", "Mind Stone", 0.72),
      candidate("sol-ring-art", "Sol Ring", 0.71),
    ]),
  )
  const { result } = renderFlow(new Map([["peer-cody", list]]))

  await waitFor(() => expect(result.current.pickerOpen).toBe(true))
  const recognition = result.current.recognition
  expect(recognition.status).toBe("done")
  if (recognition.status !== "done") return
  expect(recognition.result.candidates.map((art) => art.name)).toEqual(["Sol Ring", "Mind Stone"])
  expect(room.announceCard).not.toHaveBeenCalled()
})
