import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
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
        data: { id, name, image_uris: { normal: "https://img.example/back.jpg" } },
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
