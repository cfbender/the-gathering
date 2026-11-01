import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { PlayerSummary } from "@/features/games/games"
import type { CardSummary } from "@/lib/cards"
import { NewDeckButton } from "./new-deck-dialog"

const navigate = vi.hoisted(() => vi.fn())

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}))

const atraxa: CardSummary = {
  id: "scryfall-atraxa",
  oracle_id: "oracle-atraxa",
  name: "Atraxa, Praetors' Voice",
  mana_cost: "{G}{W}{U}{B}",
  type_line: "Legendary Creature — Phyrexian Angel Horror",
  color_identity: ["W", "U", "B", "G"],
  image_uris: {},
  can_be_commander: true,
  commander_pairing: null,
}

const player = (id: number, name: string, userId: number | null): PlayerSummary => ({
  id,
  name,
  avatar_url: null,
  user_id: userId,
  archived_at: null,
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  navigate.mockReset()
})

describe("new deck dialog", () => {
  it("creates the deck for the viewer's own player and opens it", async () => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    )
    let created: unknown
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url =
          typeof input === "string" ? input : input instanceof URL ? input.href : input.url
        const data =
          url === "/api/session"
            ? { id: 5, role: "member" }
            : url === "/api/players"
              ? // A guest the viewer may also manage is listed before their own player.
                [player(1, "Guest", null), player(2, "Someone else", 9), player(3, "Me", 5)]
              : url.startsWith("/api/cards?")
                ? [atraxa]
                : url === "/api/decks" && init?.method === "POST"
                  ? { id: 77 }
                  : []
        if (init?.method === "POST" && typeof init.body === "string")
          created = JSON.parse(init.body)
        return new Response(JSON.stringify({ data }), {
          status: 200,
          headers: { "content-type": "application/json" },
        })
      }),
    )
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <NewDeckButton />
      </QueryClientProvider>,
    )

    fireEvent.click(screen.getByRole("button", { name: "New deck" }))
    expect((await screen.findByRole("combobox", { name: "Player" })).textContent).toBe("Me (you)")
    fireEvent.change(screen.getByRole("combobox", { name: "Commander" }), {
      target: { value: "Atraxa" },
    })
    fireEvent.click(await screen.findByRole("option", { name: /Atraxa, Praetors' Voice/ }))
    fireEvent.click(screen.getByRole("button", { name: "Create deck" }))

    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({ to: "/decks/$deckId", params: { deckId: "77" } }),
    )
    expect(created).toMatchObject({
      player_id: 3,
      name: "Atraxa, Praetors' Voice",
      commander_card_id: "scryfall-atraxa",
      color_identity: "WUBG",
    })
  })
})
