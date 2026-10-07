import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { DeckSummary } from "@/features/decks/decks"
import { DecklistDialog } from "./decklist-dialog"

const deck = {
  id: 7,
  player_id: 1,
  name: "Shorikai",
  commander_card_id: null,
  commander_name: "Shorikai, Genesis Engine",
  commander_art_crop_url: null,
  partner_card_id: null,
  partner_name: null,
  partner_art_crop_url: null,
  color_identity: "WU",
  decklist_url: "https://vault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx",
  decklist_source: "manavault",
  archived_at: null,
  skip_count: 0,
  included_for_play: true,
} as DeckSummary

const TOO_OLD = "This ManaVault server is too old to share deck lists; it needs v1.3.0 or newer."
const GENERIC = "The deck site did not answer. Try again in a moment."

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

/** Every request answers `status` with `body`; returns the requested URLs. */
function stubDecklist(status: number, body: unknown) {
  const calls: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      calls.push(typeof input === "string" ? input : input instanceof URL ? input.href : input.url)
      return new Response(JSON.stringify(body), {
        status,
        headers: { "content-type": "application/json" },
      })
    }),
  )
  return calls
}

function renderDialog() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <DecklistDialog deck={deck} open onOpenChange={() => {}} boardNames={new Set()} />
    </QueryClientProvider>,
  )
}

describe("DecklistDialog failures", () => {
  it("shows the server's reason for a ManaVault too old to share decks, without Retry", async () => {
    const calls = stubDecklist(422, { errors: { detail: TOO_OLD } })
    renderDialog()

    expect((await screen.findByRole("alert")).textContent).toContain(TOO_OLD)
    expect(screen.queryByText(GENERIC)).toBeNull()
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull()
    // An explained refusal is final: the query does not retry it either.
    expect(calls).toEqual(["/api/decks/7/decklist"])
  })

  it("keeps the generic message and Retry for other failures", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const calls = stubDecklist(502, { errors: { detail: "Bad Gateway" } })
    renderDialog()
    // The query retries twice (after 1 s, then 2 s) before giving up.
    await vi.advanceTimersByTimeAsync(5000)

    const alert = await screen.findByRole("alert")
    expect(alert.textContent).toContain(GENERIC)
    expect(alert.textContent).not.toContain("Bad Gateway")
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy()
    expect(calls).toHaveLength(3)
  })
})
