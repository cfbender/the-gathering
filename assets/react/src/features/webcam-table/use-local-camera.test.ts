import { expect, it, vi } from "vite-plus/test"
import { encodeCrop, MAX_CROP_IMAGE_LENGTH } from "./use-local-camera"

/** A canvas whose JPEG grows with quality the way a noisy board's does. */
function canvasEncodingTo(lengthAt: Record<number, number>) {
  const toDataURL = vi.fn(
    (_type: string, quality: number) => `data:image/jpeg;base64,${"x".repeat(lengthAt[quality]!)}`,
  )
  return { canvas: { toDataURL } as unknown as HTMLCanvasElement, toDataURL }
}

it("keeps the first quality when the crop already fits the relay", () => {
  const { canvas, toDataURL } = canvasEncodingTo({ 0.82: 150_000 })
  expect(encodeCrop(canvas)).toHaveLength(150_000 + "data:image/jpeg;base64,".length)
  expect(toDataURL.mock.calls).toEqual([["image/jpeg", 0.82]])
})

it("re-encodes a crop that would be refused by the relay until it fits", () => {
  const { canvas, toDataURL } = canvasEncodingTo({
    0.82: 270_000,
    0.7: 210_000,
    0.55: 160_000,
    0.4: 90_000,
  })
  const image = encodeCrop(canvas)
  expect(image.length).toBeLessThanOrEqual(MAX_CROP_IMAGE_LENGTH)
  expect(toDataURL.mock.calls.map(([, quality]) => quality)).toEqual([0.82, 0.7, 0.55])
})

it("sends the coarsest encoding rather than nothing when even that is too large", () => {
  const { canvas } = canvasEncodingTo({ 0.82: 400_000, 0.7: 350_000, 0.55: 300_000, 0.4: 250_000 })
  expect(encodeCrop(canvas)).toHaveLength(250_000 + "data:image/jpeg;base64,".length)
})
