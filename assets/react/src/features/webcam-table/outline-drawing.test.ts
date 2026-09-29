import { expect, it } from "vite-plus/test"
import { placeCorner, type OutlineDraft } from "./outline-drawing"

it("Shift+click starts an outline, and three more clicks on that board finish it", () => {
  expect(placeCorner(null, "a", [0.1, 0.1], 16 / 9, false)).toEqual({ kind: "click" })
  let step = placeCorner(null, "a", [0.1, 0.1], 16 / 9, true)
  expect(step).toEqual({
    kind: "drawing",
    draft: { peerId: "a", corners: [[0.1, 0.1]], aspect: 16 / 9 },
  })
  let draft = (step as { draft: OutlineDraft }).draft
  for (const point of [
    [0.3, 0.1],
    [0.3, 0.4],
  ] as [number, number][]) {
    step = placeCorner(draft, "a", point, 16 / 9, false)
    draft = (step as { draft: OutlineDraft }).draft
  }
  // A double-click on a placed corner does not count twice.
  expect(placeCorner(draft, "a", [0.3005, 0.4], 16 / 9, false)).toEqual({ kind: "drawing", draft })
  const done = placeCorner(draft, "a", [0.1, 0.4], 16 / 9, true)
  expect(done).toMatchObject({
    kind: "done",
    peerId: "a",
    corners: [
      [0.1, 0.1],
      [0.3, 0.1],
      [0.3, 0.4],
      [0.1, 0.4],
    ],
  })
  expect((done as { centre: number[] }).centre.map((v) => Math.round(v * 100) / 100)).toEqual([
    0.2, 0.25,
  ])
})

it("a click on another board abandons the outline", () => {
  const draft: OutlineDraft = { peerId: "a", corners: [[0.1, 0.1]], aspect: 1 }
  expect(placeCorner(draft, "b", [0.5, 0.5], 1, false)).toEqual({ kind: "click" })
  expect(placeCorner(draft, "b", [0.5, 0.5], 1, true)).toMatchObject({
    kind: "drawing",
    draft: { peerId: "b", corners: [[0.5, 0.5]] },
  })
})
