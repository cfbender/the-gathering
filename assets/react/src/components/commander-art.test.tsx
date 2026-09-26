import { cleanup, fireEvent, render } from "@testing-library/react"
import { afterEach, expect, it } from "vite-plus/test"
import { CommanderArt } from "./commander-art"

afterEach(cleanup)

function images(container: HTMLElement) {
  return [...container.querySelectorAll("img")]
}

function image(container: HTMLElement, index: number) {
  const found = images(container)[index]
  if (!found) throw new Error(`No image at ${index}`)
  return found
}

it("renders nothing without art", () => {
  const { container } = render(<CommanderArt imageUrl={null} partnerImageUrl={undefined} />)
  expect(container.innerHTML).toBe("")
})

it("fills the frame with a lone commander's art", () => {
  const { container } = render(<CommanderArt imageUrl="/kangee.jpg" />)
  const lone = image(container, 0)
  expect(images(container)).toHaveLength(1)
  expect(lone.getAttribute("src")).toBe("/kangee.jpg")
  expect(lone.style.clipPath).toBe("")
})

it("keeps the commander's art whole when the partner has no crop", () => {
  const { container } = render(<CommanderArt imageUrl="/kraum.jpg" partnerImageUrl={null} />)
  expect(images(container).map((image) => image.getAttribute("src"))).toEqual(["/kraum.jpg"])
})

it("splits a partner pairing diagonally, commander on the left", () => {
  const { container } = render(<CommanderArt imageUrl="/kraum.jpg" partnerImageUrl="/tymna.jpg" />)
  const left = image(container, 0)
  const right = image(container, 1)
  expect([left.getAttribute("src"), right.getAttribute("src")]).toEqual([
    "/kraum.jpg",
    "/tymna.jpg",
  ])
  expect(left.style.left).toBe("0px")
  expect(right.style.right).toBe("0px")
  expect(left.style.clipPath).toContain("polygon(")
  expect(right.style.clipPath).toContain("polygon(")
})

it("hides a broken crop so the parent's fallback shows through", () => {
  const { container } = render(<CommanderArt imageUrl="/kraum.jpg" partnerImageUrl="/tymna.jpg" />)
  const right = image(container, 1)
  fireEvent.error(right)
  expect(right.hidden).toBe(true)
})
