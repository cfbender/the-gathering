import { expect, it } from "vite-plus/test"
import { cameraRailFitWidth } from "./use-camera-rail-width"

it("grows the rail by the board's pillarbox slack", () => {
  // A 1600×720 board shows a 16:9 video 1280 wide, leaving 320px of bars.
  expect(cameraRailFitWidth(240, [{ width: 1600, height: 720, aspect: 16 / 9 }], 2560)).toBe(560)
  // The fit holds still as the rail takes that width from the board.
  expect(cameraRailFitWidth(560, [{ width: 1280, height: 720, aspect: 16 / 9 }], 2560)).toBe(560)
})

it("keeps the original min(360px, 24vw) limit when the board is height-bound", () => {
  const tall = [{ width: 1000, height: 900, aspect: 16 / 9 }]
  expect(cameraRailFitWidth(240, tall, 1920)).toBe(360)
  expect(cameraRailFitWidth(240, tall, 1200)).toBe(288)
})

it("fits the tightest board and follows each video's own aspect", () => {
  const boards = [
    { width: 1600, height: 400, aspect: 16 / 9 },
    { width: 1600, height: 400, aspect: 4 / 3 },
  ]
  // 16:9 at 400 tall is 711 wide, the tighter of the two boards.
  expect(cameraRailFitWidth(240, boards, 2560)).toBe(1128)
})

it("has no fit without a stage board", () => {
  expect(cameraRailFitWidth(240, [], 1920)).toBeNull()
})
