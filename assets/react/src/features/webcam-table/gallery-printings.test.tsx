import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { CardSuggestions } from "./card-suggestions"
import { CardsTab } from "./cards-tab"
import type { Identification } from "./recognition/messages"
import { searchArts, type GalleryArt } from "./recognition/pipeline"
import { EMPTY_COUNTERS } from "./seat-counters"
import { correctionPayload } from "./use-correction-upload"
import type { CapturedCard } from "./use-webcam-room"

const art: GalleryArt = {
  id: "18052761-39c3-4342-b6e6-38dc5b7b05dd",
  name: "Sol Talisman",
  set: "mh2",
  collector_number: "472",
  frame: "extended",
  printings: [
    {
      id: "a51fb64d-cc0c-400d-971f-78c28d42043b",
      name: "Sol Talisman",
      set: "mh2",
      collector_number: "236",
      lang: "en",
    },
    {
      id: "18052761-39c3-4342-b6e6-38dc5b7b05dd",
      name: "Sol Talisman",
      set: "mh2",
      collector_number: "472",
      lang: "en",
      frame_effects: ["extendedart"],
    },
  ],
}
const capture: CapturedCard = {
  peerId: "owner",
  playerId: 1,
  image: "data:image/jpeg;base64,/9j/",
  nativeWidth: 1920,
  nativeHeight: 1080,
  cropSize: 640,
  clickX: 120,
  clickY: 220,
  inspect: true,
  private: false,
  shareCorrections: true,
}
const result: Identification = {
  candidates: [{ ...art, score: 0.8, index: 0 }],
  quad: [
    [0, 0],
    [100, 0],
    [100, 140],
    [0, 140],
  ],
  upVote: 1,
  timings: { detector: 1, embed: 2, search: 3, total: 6 },
}
const search = async (query: string) => searchArts([art], query)

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  localStorage.clear()
})

it("offers regular and extended siblings under one ranked candidate and tags the exact selection", () => {
  const choose = vi.fn()
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CardSuggestions
        capture={capture}
        playerName="Cody"
        recognition={{ status: "done", result }}
        deckSuggestions={[]}
        gallerySearchable
        onChooseCard={choose}
        onChooseDeck={vi.fn()}
        onSearch={search}
        onDismiss={vi.fn()}
      />
    </QueryClientProvider>,
  )
  expect(document.querySelectorAll("kbd")).toHaveLength(1)
  fireEvent.click(screen.getByText("2 printings of Sol Talisman"))
  const choices = screen.getByRole("list", { name: "Printings of Sol Talisman" })
  expect(within(choices).getByRole("button", { name: "MH2 #472 · EN · extendedart" })).toBeTruthy()
  fireEvent.click(within(choices).getByRole("button", { name: "MH2 #236 · EN" }))
  expect(choose).toHaveBeenCalledWith(
    expect.objectContaining({
      id: "a51fb64d-cc0c-400d-971f-78c28d42043b",
      collector_number: "236",
    }),
  )
  expect(
    correctionPayload("capture", capture, result, choose.mock.calls[0]?.[0].id, "new-bundle", true),
  ).toMatchObject({
    label: "a51fb64d-cc0c-400d-971f-78c28d42043b",
    top1: art.id,
  })
})

it("searches and previews an exact sibling in the Cards tab", async () => {
  const preview = vi.fn()
  const participant = {
    ...EMPTY_COUNTERS,
    peer_id: "owner",
    player_id: 1,
    player_name: "Cody",
    life: 40,
    joined_at: 0,
    camera_off: true,
    eliminated: false,
  }
  render(
    <CardsTab
      participants={[participant]}
      localParticipant={participant}
      identifiedCards={[]}
      gallerySearchable
      onSearch={search}
      onPreviewArt={preview}
      onPreviewCard={vi.fn()}
      onRemoveCard={vi.fn()}
      onClearOwnCards={vi.fn()}
    />,
  )
  fireEvent.change(screen.getByRole("textbox", { name: "Search the card gallery" }), {
    target: { value: "sol mh2 236 lang:en" },
  })
  fireEvent.click(await screen.findByRole("button", { name: /Sol Talisman\s*MH2 #236 · EN/ }))
  expect(preview).toHaveBeenCalledWith(
    expect.objectContaining({ id: "a51fb64d-cc0c-400d-971f-78c28d42043b", lang: "en" }),
  )
})

it("picker search uses a sibling's own details for hover and selects that printing", async () => {
  const id = "a51fb64d-cc0c-400d-971f-78c28d42043b"
  const fetch = vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(
      JSON.stringify({
        data: {
          id,
          name: "Sol Talisman",
          image_uris: { normal: "https://img.example/regular.jpg" },
          prices: { usd: "0.99", usd_foil: null, usd_etched: null },
        },
      }),
    ),
  )
  const choose = vi.fn()
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CardSuggestions
        capture={capture}
        playerName="Cody"
        recognition={{ status: "done", result }}
        deckSuggestions={[]}
        gallerySearchable
        onChooseCard={choose}
        onChooseDeck={vi.fn()}
        onSearch={search}
        onDismiss={vi.fn()}
      />
    </QueryClientProvider>,
  )
  fireEvent.change(screen.getByRole("textbox", { name: "Search the card gallery" }), {
    target: { value: "sol 236" },
  })
  const button = await screen.findByRole("button", { name: /Sol Talisman\s*MH2 #236 · EN/ })
  fireEvent.mouseEnter(button)
  expect((await screen.findByRole("img", { name: "Sol Talisman" })).getAttribute("src")).toBe(
    "https://img.example/regular.jpg",
  )
  expect(fetch).toHaveBeenCalledWith(`/api/card-printings/${id}/details`, expect.anything())
  fireEvent.click(button)
  expect(choose).toHaveBeenCalledWith(expect.objectContaining({ id }))
})

it("labels different card names sharing one illustration rather than disguising them as reprints", () => {
  const choose = vi.fn()
  const killbot = {
    ...result.candidates[0]!,
    name: "Curious Killbot",
    printings: [
      { id: "curious", name: "Curious Killbot", set: "ust", collector_number: "147a", lang: "en" },
      {
        id: "delighted",
        name: "Delighted Killbot",
        set: "ust",
        collector_number: "147b",
        lang: "en",
      },
    ],
  }
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CardSuggestions
        capture={capture}
        playerName="Cody"
        recognition={{ status: "done", result: { ...result, candidates: [killbot] } }}
        deckSuggestions={[]}
        gallerySearchable
        onChooseCard={choose}
        onChooseDeck={vi.fn()}
        onSearch={search}
        onDismiss={vi.fn()}
      />
    </QueryClientProvider>,
  )
  fireEvent.click(screen.getByText("2 printings of Curious Killbot"))
  fireEvent.click(screen.getByRole("button", { name: /Delighted Killbot\s*UST #147b · EN/ }))
  expect(choose).toHaveBeenCalledWith(
    expect.objectContaining({ id: "delighted", name: "Delighted Killbot" }),
  )
})
