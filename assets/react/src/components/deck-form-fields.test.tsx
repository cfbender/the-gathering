import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { cardSnapshot, type CardSummary } from "@/lib/cards"
import { CommanderField } from "./commander-field"
import { DeckFormFields, type DeckFormValue } from "./deck-form-fields"

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

function Harness() {
  const [name, setName] = useState("")
  const [value, setValue] = useState<DeckFormValue>({
    commander: null,
    partner: null,
    colorIdentity: "",
    decklistUrl: "",
  })
  return (
    <>
      <DeckFormFields
        value={value}
        onChange={(patch) => setValue((current) => ({ ...current, ...patch }))}
        onResolvedName={setName}
      />
      <output data-testid="deck-name">{name}</output>
      <output data-testid="commander-id">{value.commander?.catalog_id}</output>
      <output data-testid="commander-name">{value.commander?.name}</output>
      <output data-testid="partner-id">{value.partner?.catalog_id}</output>
      <output data-testid="colors">{value.colorIdentity}</output>
      <output data-testid="decklist-url">{value.decklistUrl}</output>
    </>
  )
}

function renderWithQueryClient(component = <Harness />) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(<QueryClientProvider client={queryClient}>{component}</QueryClientProvider>)
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("deck card fields", () => {
  it("stores the selected commander ID and prefills color identity", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation(() =>
        Promise.resolve(
          new Response(JSON.stringify({ data: [atraxa] }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
        ),
      ),
    )
    renderWithQueryClient()

    const commander = screen.getByRole("combobox", { name: "Commander" })
    fireEvent.change(commander, { target: { value: "Atraxa" } })
    fireEvent.click(await screen.findByRole("option", { name: /Atraxa, Praetors' Voice/ }))

    expect(screen.getByTestId("commander-id").textContent).toBe("scryfall-atraxa")
    expect(screen.getByTestId("colors").textContent).toBe("WUBG")
  })

  it("resolves a deck list, looks up exact commanders, and falls back to a name snapshot", async () => {
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      const data = url.startsWith("/api/decklists/resolve")
        ? {
            source: "moxfield",
            id: "deck-id",
            url: "https://moxfield.com/decks/deck-id",
            name: "Imported counters",
            commanders: [{ name: atraxa.name }, { name: "Missing Partner" }],
            color_identity: ["W", "U", "B", "G"],
            fetched_at: "2026-09-19T12:00:00Z",
          }
        : url.includes("q=Atraxa")
          ? [atraxa]
          : []
      return new Response(JSON.stringify({ data }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    })
    vi.stubGlobal("fetch", fetch)
    renderWithQueryClient()

    const url = screen.getByRole("textbox", { name: "Deck-list URL" })
    fireEvent.change(url, { target: { value: "https://moxfield.com/decks/deck-id" } })
    fireEvent.blur(url)
    fireEvent.click(await screen.findByRole("button", { name: /Use these details/ }))

    await waitFor(() =>
      expect(screen.getByTestId("deck-name").textContent).toBe("Imported counters"),
    )
    expect(screen.getByTestId("commander-id").textContent).toBe("scryfall-atraxa")
    expect(screen.getByTestId("partner-id").textContent).toBe("")
    expect(screen.getByTestId("colors").textContent).toBe("WUBG")
  })

  it("resolves a hosted-deck quick pick before prefilling the deck", async () => {
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url

      if (url === "/api/session/remote-decks") {
        return new Response(
          JSON.stringify({
            data: {
              decks: [
                {
                  name: "Hosted counters",
                  commanders: [atraxa.name],
                  color_identity: ["W", "U", "B", "G"],
                  url: "https://moxfield.com/decks/hosted",
                  source: "moxfield",
                  updated_at: "2026-09-20T12:00:00Z",
                },
              ],
              sources: [{ source: "moxfield", configured: true, error: null }],
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        )
      }

      if (url === "/api/decklists/resolve") {
        return new Response(
          JSON.stringify({
            data: {
              source: "moxfield",
              id: "hosted",
              url: "https://moxfield.com/decks/hosted",
              name: "Hosted counters",
              commanders: [{ name: atraxa.name }],
              color_identity: ["W", "U", "B", "G"],
              fetched_at: "2026-09-20T12:00:00Z",
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        )
      }

      return new Response(JSON.stringify({ data: [atraxa] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    })
    vi.stubGlobal("fetch", fetch)
    renderWithQueryClient()

    const picker = await screen.findByRole("combobox", {
      name: "Quick pick from my hosted decks",
    })
    fireEvent.change(picker, { target: { value: "https://moxfield.com/decks/hosted" } })

    await waitFor(() => expect(screen.getByTestId("deck-name").textContent).toBe("Hosted counters"))
    expect(screen.getByTestId("commander-id").textContent).toBe("scryfall-atraxa")
    expect(screen.getByTestId("colors").textContent).toBe("WUBG")
    expect(screen.getByTestId("decklist-url").textContent).toBe("https://moxfield.com/decks/hosted")
    expect((picker as HTMLSelectElement).value).toBe("https://moxfield.com/decks/hosted")
    expect(fetch).toHaveBeenCalledWith(
      "/api/decklists/resolve",
      expect.objectContaining({ method: "POST" }),
    )
  })

  it("prefills a ManaVault quick pick from the listing without resolving a share link", async () => {
    const fetch = vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url

      if (url === "/api/session/remote-decks") {
        return new Response(
          JSON.stringify({
            data: {
              decks: [
                {
                  name: "Vault counters",
                  commanders: [atraxa.name],
                  color_identity: ["W", "U", "B", "G"],
                  url: "https://vault.example.com/decks/42",
                  source: "manavault",
                  updated_at: "2026-09-20T12:00:00Z",
                },
              ],
              sources: [{ source: "manavault", configured: true, error: null }],
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        )
      }

      if (url === "/api/decklists/resolve") {
        return new Response(JSON.stringify({ errors: { detail: "Unsupported" } }), {
          status: 422,
          headers: { "content-type": "application/json" },
        })
      }

      return new Response(JSON.stringify({ data: [atraxa] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    })
    vi.stubGlobal("fetch", fetch)
    renderWithQueryClient()

    const picker = await screen.findByRole("combobox", {
      name: "Quick pick from my hosted decks",
    })
    fireEvent.change(picker, { target: { value: "https://vault.example.com/decks/42" } })

    await waitFor(() => expect(screen.getByTestId("deck-name").textContent).toBe("Vault counters"))
    expect(screen.getByTestId("commander-id").textContent).toBe("scryfall-atraxa")
    expect(screen.getByTestId("colors").textContent).toBe("WUBG")
    expect(screen.getByTestId("decklist-url").textContent).toBe(
      "https://vault.example.com/decks/42",
    )
    expect(fetch).not.toHaveBeenCalledWith("/api/decklists/resolve", expect.anything())
  })

  it("renders a stored name when its card ID is unavailable", () => {
    renderWithQueryClient(
      <CommanderField
        value={cardSnapshot("deleted-scryfall-id", "Legacy Commander")}
        onChange={() => undefined}
      />,
    )

    expect(screen.getByRole("combobox", { name: "Commander" }).getAttribute("value")).toBe(
      "Legacy Commander",
    )
    expect(screen.getByText("Stored card snapshot")).toBeTruthy()
  })
})
