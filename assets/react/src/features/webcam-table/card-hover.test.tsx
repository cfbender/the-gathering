import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { CommanderHover } from "./card-hover"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

it.each(["https://img.example/selected-printing.jpg", null, undefined])(
  "uses the serialized commander image %s, or the art crop, without fetching details",
  (imageUrl) => {
    const fetch = vi.spyOn(globalThis, "fetch")
    const deck = {
      commander_name: "Kangee, Sky Warden",
      commander_image_url: imageUrl,
      commander_art_crop_url: "https://img.example/selected-art.jpg",
    }
    render(
      <QueryClientProvider client={new QueryClient()}>
        <CommanderHover deck={deck}>
          <button type="button">Commander</button>
        </CommanderHover>
      </QueryClientProvider>,
    )
    fireEvent.mouseEnter(screen.getByRole("button", { name: "Commander" }))
    expect(screen.getByRole("img", { name: deck.commander_name }).getAttribute("src")).toBe(
      imageUrl ?? "https://img.example/selected-art.jpg",
    )
    expect(fetch).not.toHaveBeenCalled()
  },
)
