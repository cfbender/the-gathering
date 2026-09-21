import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, it } from "vite-plus/test"
import { CSVCorrectionReview, materialChanges } from "./csv-correction-review"
import type { CSVImportReview } from "./imports"

afterEach(cleanup)

const review: CSVImportReview[] = [
  {
    game_id: "changed",
    action: "update",
    target_id: null,
    changes: [
      { field: "turns", player: null, before: 8, after: 9 },
      { field: "notes", player: null, before: "same", after: "same" },
    ],
  },
  { game_id: "unchanged", action: "skip", target_id: null, changes: [] },
]

it("shows only material differences and hides unchanged games by default", () => {
  const [changed] = review
  if (!changed) throw new Error("Missing changed review fixture")
  render(<CSVCorrectionReview review={review} />)
  expect(screen.getByText("changed")).toBeTruthy()
  expect(screen.queryByText("unchanged")).toBeNull()
  fireEvent.click(screen.getByRole("checkbox", { name: /show all/i }))
  expect(screen.getByText("unchanged")).toBeTruthy()
  expect(materialChanges(changed)).toEqual([changed.changes[0]])
})
