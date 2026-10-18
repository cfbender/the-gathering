import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import type { ScriptPreview } from "./deck-chooser"
import { DeckScriptPage } from "./deck-script-page"
import { DECK_SCRIPT_REFERENCE } from "./deck-script-reference"

const context = {
  now: { hour: 22, minute: 5, weekday: 6, weekday_name: "friday", weekend: false },
  timezone: "America/New_York",
  games_today: 2,
  session_games: 3,
  minutes_since_last_game: 12,
  last_game: { result: "loss", deck_name: "Krenko", duration_minutes: 25, hours_ago: 0.2 },
  recent_colors: { W: 0, U: 1, B: 1, R: 4, G: 0, C: 0 },
  stats: { games: 9, wins: 3, losses: 6, draws: 0, win_rate: 0.33, decks: 2 },
}

function preview(script: string | null): ScriptPreview {
  const scripted = script?.includes("rematch") ?? false
  return {
    reason: null,
    source: scripted ? "script" : "built_in",
    error: null,
    logs: scripted ? ["Krenko\t4"] : [],
    context,
    decks: [
      {
        id: 1,
        name: "Krenko",
        base_weight: 10,
        weight: scripted ? 30 : 10,
        chance: scripted ? 0.75 : 0.5,
        note: scripted ? "rematch x3" : null,
        input: { name: "Krenko", color_identity: "R", plays: 4, skips: 2, wins: 1, losses: 3 },
      },
      {
        id: 2,
        name: "Atraxa",
        base_weight: 10,
        weight: 10,
        chance: scripted ? 0.25 : 0.5,
        note: null,
        input: { name: "Atraxa", color_identity: "WUBG", plays: 0, skips: 0, wins: 0, losses: 0 },
      },
    ],
  }
}

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })

function stubApi(onSave: (script: string | null) => Response) {
  const requests: Array<{ method: string; url: string; body: { script?: string | null } }> = []
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url
      const method = init?.method ?? "GET"
      const body = (typeof init?.body === "string" ? JSON.parse(init.body) : {}) as {
        script?: string | null
      }
      requests.push({ method, url, body })
      if (url === "/api/deck-chooser/preview") return json({ data: preview(body.script ?? null) })
      if (method === "PUT") return onSave(body.script ?? null)
      return json({ data: { script: null } })
    }),
  )
  return requests
}

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  render(
    <QueryClientProvider client={client}>
      <DeckScriptPage />
    </QueryClientProvider>,
  )
}

const draft = `function weight(deck, ctx)
  return deck.base_weight * 3, "rematch"
end`

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("deck chooser script editor", () => {
  it("previews each edit against the member's decks, then saves the draft", async () => {
    const requests = stubApi((script) => json({ data: { script } }))
    renderPage()

    const editor = await screen.findByLabelText<HTMLTextAreaElement>("Deck chooser script")
    expect(editor.value).toContain("function weight(deck, ctx)")
    expect(await screen.findByText("Built-in weights")).toBeTruthy()
    expect(screen.getByText("Friday 10:05 PM")).toBeTruthy()
    expect(screen.getByText("Lost with Krenko · 25 min · Just now")).toBeTruthy()

    fireEvent.change(editor, { target: { value: draft } })
    expect(await screen.findByText("Script weights")).toBeTruthy()
    const krenko = screen.getByRole("button", { name: "Krenko" }).closest("tr")
    expect(krenko).toBeTruthy()
    expect(within(krenko as HTMLElement).getByText("rematch x3")).toBeTruthy()
    expect(within(krenko as HTMLElement).getByText("75%")).toBeTruthy()
    expect(screen.getByLabelText("Script log").textContent).toContain("Krenko\t4")
    const previews = requests.filter((request) => request.url === "/api/deck-chooser/preview")
    expect(previews.at(-1)?.body.script).toBe(draft)

    fireEvent.click(screen.getByRole("button", { name: "Krenko" }))
    expect(screen.getByLabelText("Krenko deck table").textContent).toContain('color_identity = "R"')

    fireEvent.click(screen.getByRole("button", { name: "Save script" }))
    await screen.findByText("your saved script")
    expect(requests.find((request) => request.method === "PUT")?.body).toEqual({ script: draft })
  })

  it("shows why a script could not be saved and jumps to its line", async () => {
    stubApi(() => json({ errors: { script: ["line 2: unexpected symbol near 'end'"] } }, 422))
    renderPage()
    const editor = await screen.findByLabelText<HTMLTextAreaElement>("Deck chooser script")
    fireEvent.change(editor, { target: { value: "function weight(deck)\n  return (\nend" } })
    fireEvent.click(screen.getByRole("button", { name: "Save script" }))

    expect(
      await screen.findByText("Could not save: line 2: unexpected symbol near 'end'"),
    ).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Line 2" }))
    await waitFor(() => expect(editor.selectionStart).toBe("function weight(deck)\n".length))
  })
})

describe("deck chooser script reference", () => {
  it("reads every documented table and field from the guide", () => {
    expect(DECK_SCRIPT_REFERENCE.map((section) => section.name)).toEqual([
      "deck",
      "game",
      "opponent",
      "time",
      "ctx",
    ])
    for (const section of DECK_SCRIPT_REFERENCE) {
      expect(section.intro).not.toBe("")
      expect(section.fields.length).toBeGreaterThan(5)
      for (const field of section.fields) {
        expect(field.name.startsWith(`${section.name}.`)).toBe(true)
        expect(field.type).not.toBe("")
        expect(field.description).not.toBe("")
      }
    }
    const deck = DECK_SCRIPT_REFERENCE[0]?.fields.find((field) => field.name === "deck.base_weight")
    expect(deck?.type).toBe("number")
  })
})
