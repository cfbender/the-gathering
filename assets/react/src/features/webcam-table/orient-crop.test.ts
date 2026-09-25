import { afterEach, expect, it, vi } from "vite-plus/test"
import { NO_FLIP } from "./board"
import { orientCrop } from "./orient-crop"

const crop = { image: "data:image/jpeg;base64,native", cropSize: 640, clickX: 100, clickY: 200 }

function fakeCanvas() {
  const context = { setTransform: vi.fn(), drawImage: vi.fn() }
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    context as unknown as CanvasRenderingContext2D,
  )
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/jpeg;base64,seen")
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    value: () => Promise.resolve(),
  })
  return context
}

afterEach(() => {
  vi.restoreAllMocks()
})

it("passes an unflipped crop through untouched", async () => {
  expect(await orientCrop(crop, NO_FLIP)).toBe(crop)
})

it.each([
  [{ vertical: true, horizontal: false }, [1, 0, 0, -1, 0, 640], { clickX: 100, clickY: 440 }],
  [{ vertical: false, horizontal: true }, [-1, 0, 0, 1, 640, 0], { clickX: 540, clickY: 200 }],
  [{ vertical: true, horizontal: true }, [-1, 0, 0, -1, 640, 640], { clickX: 540, clickY: 440 }],
])("mirrors the pixels and the click for %j", async (flip, transform, click) => {
  const context = fakeCanvas()
  expect(await orientCrop(crop, flip)).toEqual({
    image: "data:image/jpeg;base64,seen",
    cropSize: 640,
    ...click,
  })
  expect(context.setTransform).toHaveBeenCalledWith(...transform)
  expect(context.drawImage).toHaveBeenCalledWith(expect.any(HTMLImageElement), 0, 0, 640, 640)
})
