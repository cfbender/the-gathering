import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { useState } from "react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { DecklistUrlField } from "./decklist-url-field"

function Field() {
  const [value, setValue] = useState("")
  return <DecklistUrlField value={value} onChange={setValue} />
}

function renderField() {
  const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={queryClient}>
      <Field />
    </QueryClientProvider>,
  )
}

function setManavaultUrl(url: string | null) {
  document.querySelector('meta[name="manavault-url"]')?.remove()
  if (!url) return
  const meta = document.createElement("meta")
  meta.name = "manavault-url"
  meta.content = url
  document.head.appendChild(meta)
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  setManavaultUrl(null)
})

describe("DecklistUrlField", () => {
  it.each([
    ["https://www.moxfield.com/decks/abc?x=1", "Moxfield"],
    ["https://archidekt.com/decks/123/a-deck", "Archidekt"],
    ["https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx", "ManaVault"],
    ["https://example.com/deck", "Deck link"],
  ])("recognizes %s as %s", (url, label) => {
    setManavaultUrl("https://manavault.example.com")
    renderField()
    fireEvent.change(screen.getByRole("textbox", { name: "Deck-list URL" }), {
      target: { value: url },
    })
    expect(screen.getByText(label)).toBeTruthy()
  })

  it("labels ManaVault links as plain deck links when no instance is configured", () => {
    renderField()
    fireEvent.change(screen.getByRole("textbox", { name: "Deck-list URL" }), {
      target: { value: "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx" },
    })
    expect(screen.getByText("Deck link")).toBeTruthy()
  })

  it("shows the API's inline URL error", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ errors: { url: ["is not a supported deck-list URL"] } }), {
          status: 422,
          headers: { "content-type": "application/json" },
        }),
      ),
    )
    renderField()
    const input = screen.getByRole("textbox", { name: "Deck-list URL" })
    fireEvent.change(input, { target: { value: "https://example.com/deck" } })
    fireEvent.blur(input)

    expect((await screen.findByRole("alert")).textContent).toContain(
      "is not a supported deck-list URL",
    )
  })
})
