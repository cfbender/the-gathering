import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it } from "vite-plus/test"
import { DeckCommanders } from "./deck-commanders"

afterEach(cleanup)

const deck = {
  commander_name: "Thrasios, Triton Hero",
  commander_game_changer: false,
  commander_image_url: "https://img.example/thrasios.jpg",
  commander_art_crop_url: null,
  partner_name: "Tymna the Weaver",
  partner_game_changer: false,
  partner_image_url: "https://img.example/tymna.jpg",
  partner_art_crop_url: null,
}

it("previews the hovered commander or partner card", () => {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <DeckCommanders deck={deck} hover />
    </QueryClientProvider>,
  )
  expect(screen.queryByRole("img")).toBeNull()

  fireEvent.mouseEnter(screen.getByText("Tymna the Weaver"))
  expect(screen.getByRole("img", { name: "Tymna the Weaver" }).getAttribute("src")).toBe(
    "https://img.example/tymna.jpg",
  )
  expect(screen.queryByRole("img", { name: "Thrasios, Triton Hero" })).toBeNull()
})

it("renders plain names without hover", () => {
  render(<DeckCommanders deck={deck} />)
  fireEvent.mouseEnter(screen.getByText("Tymna the Weaver"))
  expect(screen.queryByRole("img")).toBeNull()
})

it("stacks each commander on its own line without a separator", () => {
  const { container } = render(<DeckCommanders deck={deck} stacked />)
  expect(screen.getByText("Thrasios, Triton Hero")).toBeTruthy()
  expect(screen.getByText("Tymna the Weaver")).toBeTruthy()
  expect(container.textContent).not.toContain("/")
})
