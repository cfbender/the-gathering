import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { CardsTab } from "./cards-tab"
import { EMPTY_COUNTERS } from "./seat-counters"
import type { BoardCard, TableParticipant } from "./use-webcam-room"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function participant(peer_id: string, player_name: string): TableParticipant {
  return {
    ...EMPTY_COUNTERS,
    peer_id,
    player_id: 1,
    player_name,
    life: 40,
    joined_at: 0,
    camera_off: true,
    eliminated: false,
  }
}

function boardCard(id: string, ownerPeerId: string, name: string, at: number): BoardCard {
  return {
    id,
    ownerPeerId,
    byPlayerName: "Cody",
    card: { id: `${id}-printing`, name, set: "lea", collector_number: "1" },
    at,
  }
}

function renderTab() {
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
    const id = (input as string).split("/")[3]
    return new Response(
      JSON.stringify({
        data: {
          id,
          name: id,
          set_code: "lea",
          collector_number: "1",
          image_uris: { normal: `https://img.example/${id}.jpg` },
          game_changer: id === "bolt-printing",
          prices: { usd: null, usd_foil: null, usd_etched: null },
        },
      }),
    )
  })
  const cody = participant("cody", "Cody")
  const alex = participant("alex", "Alex")
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <CardsTab
        participants={[cody, alex]}
        localParticipant={cody}
        identifiedCards={[
          boardCard("bolt", "cody", "Lightning Bolt", 1),
          boardCard("counter", "alex", "Counterspell", 2),
        ]}
        gallerySearchable={false}
        onSearch={vi.fn()}
        onPreviewArt={vi.fn()}
        onPreviewCard={vi.fn()}
        onRemoveCard={vi.fn()}
        onClearOwnCards={vi.fn()}
      />
    </QueryClientProvider>,
  )
}

it("collapses each player's detected cards independently", () => {
  renderTab()
  const alex = screen.getByRole("region", { name: "Cards on Alex's board" })
  const cody = screen.getByRole("region", { name: "Cards on Cody's board" })
  const toggle = within(alex).getByRole("button", { name: "Alex (1)" })
  expect(toggle.getAttribute("aria-expanded")).toBe("true")

  fireEvent.click(toggle)
  expect(toggle.getAttribute("aria-expanded")).toBe("false")
  expect(within(alex).queryByRole("button", { name: /Counterspell/ })).toBeNull()
  expect(within(cody).getByRole("button", { name: "Remove Lightning Bolt" })).toBeTruthy()
  expect(within(cody).getByRole("button", { name: "Clear cards" })).toBeTruthy()

  fireEvent.click(toggle)
  expect(within(alex).getByRole("button", { name: "Remove Counterspell" })).toBeTruthy()
})

it("shows a large card image when hovering a detected card", async () => {
  renderTab()
  const cody = screen.getByRole("region", { name: "Cards on Cody's board" })
  fireEvent.mouseEnter(within(cody).getAllByRole("button", { name: /Lightning Bolt/ })[0]!)
  const preview = await screen.findByRole("dialog", { name: "Lightning Bolt image preview" })
  const image = await within(preview).findByRole("img", { name: "Lightning Bolt" })
  expect(image.getAttribute("src")).toBe("https://img.example/bolt-printing.jpg")
})

it("marks Game Changers with a compact badge whose tooltip names it", async () => {
  renderTab()
  const cody = screen.getByRole("region", { name: "Cards on Cody's board" })
  const badges = await within(cody).findAllByTitle(/^Game Changer/)
  for (const badge of badges) {
    expect(badge.textContent).toBe("Game Changer")
    expect(badge.querySelector(".sr-only")).toBeTruthy()
  }
  const alex = screen.getByRole("region", { name: "Cards on Alex's board" })
  expect(within(alex).queryByTitle(/^Game Changer/)).toBeNull()
})
