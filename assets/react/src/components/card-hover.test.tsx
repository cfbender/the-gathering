import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { CardHover, CommanderHover } from "./card-hover"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

it("fetches the suffixed face ID and displays the face image and name", async () => {
  const id = "b0a96416-9ee5-4202-a99f-e09db8794567-1"
  const name = "Journey to the Oracle"
  const fetch = vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(
      JSON.stringify({
        data: {
          id,
          name,
          image_uris: { normal: "https://img.example/back.jpg" },
          prices: { usd: "0.25", usd_foil: null, usd_etched: null },
        },
      }),
    ),
  )
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <CardHover id={id} name={name}>
        <button type="button">Back face</button>
      </CardHover>
    </QueryClientProvider>,
  )
  fireEvent.mouseEnter(screen.getByRole("button", { name: "Back face" }))
  const image = await screen.findByRole("img", { name })
  expect(image.getAttribute("src")).toBe("https://img.example/back.jpg")
  expect(fetch).toHaveBeenCalledWith(`/api/card-printings/${id}/details`, expect.anything())
  expect(screen.getByText(name)).toBeTruthy()
  expect(screen.getByText("$0.25")).toBeTruthy()
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

it("previews both commanders side by side for a partner pairing", () => {
  const fetch = vi.spyOn(globalThis, "fetch")
  const deck = {
    commander_name: "Kraum, Ludevic's Opus",
    commander_image_url: "https://img.example/kraum.jpg",
    commander_art_crop_url: null,
    partner_name: "Tymna the Weaver",
    partner_image_url: "https://img.example/tymna.jpg",
    partner_art_crop_url: null,
  }
  render(
    <QueryClientProvider client={new QueryClient()}>
      <CommanderHover deck={deck}>
        <button type="button">Commander</button>
      </CommanderHover>
    </QueryClientProvider>,
  )
  fireEvent.mouseEnter(screen.getByRole("button", { name: "Commander" }))
  const preview = screen.getByRole("dialog", {
    name: "Kraum, Ludevic's Opus and Tymna the Weaver image preview",
  })
  expect(within(preview).getByRole("img", { name: deck.commander_name }).getAttribute("src")).toBe(
    "https://img.example/kraum.jpg",
  )
  expect(within(preview).getByRole("img", { name: deck.partner_name }).getAttribute("src")).toBe(
    "https://img.example/tymna.jpg",
  )
  expect(fetch).not.toHaveBeenCalled()
})

it("shows printing prices on focus without moving focus into the hover card", () => {
  const client = new QueryClient()
  client.setQueryData(["card-printings", "forest", "details"], {
    image_uris: { normal: "https://img.example/forest.jpg" },
    prices: { usd: "0.25", usd_foil: "1.10", usd_etched: null },
  })
  render(
    <QueryClientProvider client={client}>
      <CardHover id="forest" name="Forest">
        <button>Forest</button>
      </CardHover>
    </QueryClientProvider>,
  )
  const trigger = screen.getByRole("button", { name: "Forest" })
  act(() => trigger.focus())
  expect(screen.getByText("$0.25 · Foil $1.10")).toBeTruthy()
  expect(screen.getByRole("img", { name: "Forest" }).getAttribute("src")).toBe(
    "https://img.example/forest.jpg",
  )
  expect(document.activeElement).toBe(trigger)
})
