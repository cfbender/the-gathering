import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vite-plus/test"
import { ColorIdentity, ManaCost, parseManaCost } from "./mana-symbols"

afterEach(cleanup)

describe("mana symbols", () => {
  it("parses hybrid, phyrexian, multi-digit generic, variable, and utility symbols", () => {
    const parts = parseManaCost("{G/W}{G/P}{10}{X}{C}{S}{T}")

    expect(parts.map((part) => (part.kind === "symbol" ? part.token : part.text))).toEqual([
      "{G/W}",
      "{G/P}",
      "{10}",
      "{X}",
      "{C}",
      "{S}",
      "{T}",
    ])
    expect(parts.every((part) => part.kind === "symbol")).toBe(true)
  })

  it("keeps an unknown token visible as text", () => {
    render(<ManaCost cost="{G}{NOPE}" />)

    expect(screen.getByRole("img", { name: "green mana" })).toBeTruthy()
    expect(screen.getByText("{NOPE}")).toBeTruthy()
  })

  it("orders color identity as WUBRG regardless of input order", () => {
    render(<ColorIdentity colors="GRBUW" />)

    expect(screen.getAllByRole("img").map((image) => image.getAttribute("alt"))).toEqual([
      "white mana",
      "blue mana",
      "black mana",
      "red mana",
      "green mana",
    ])
  })

  it("renders an empty identity as a colorless pip", () => {
    render(<ColorIdentity colors="" />)

    expect(screen.getByLabelText("Color identity: colorless")).toBeTruthy()
    expect(screen.getByRole("img", { name: "colorless mana" })).toBeTruthy()
  })
})
