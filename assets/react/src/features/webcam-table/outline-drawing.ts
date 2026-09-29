import { useCallback, useEffect, useRef, useState } from "react"
import type { Point } from "./recognition/pipeline"

/** A card outline being drawn on one board: corners as fractions of its camera frame. */
export interface OutlineDraft {
  peerId: string
  corners: Point[]
  /** The frame's width / height, to draw the corners over an `object-contain` video. */
  aspect: number
}

/** What a click on a board means while outlines can be drawn. */
export type CornerStep =
  /** An ordinary click: identify the card under it. */
  | { kind: "click" }
  | { kind: "drawing"; draft: OutlineDraft }
  /** The fourth corner: crop around `centre` and identify the card inside `corners`. */
  | { kind: "done"; peerId: string; corners: Point[]; centre: Point }

/** A second click this close to a corner (fraction of the frame) is a double-click, not a corner. */
const SAME_CORNER = 0.004

/**
 * Shift+click starts an outline; the next three clicks on that board, with or without Shift,
 * place the other corners in any order. A click on another board abandons the outline.
 */
export function placeCorner(
  draft: OutlineDraft | null,
  peerId: string,
  point: Point,
  aspect: number,
  shift: boolean,
): CornerStep {
  if (draft?.peerId !== peerId) {
    return shift
      ? { kind: "drawing", draft: { peerId, corners: [point], aspect } }
      : { kind: "click" }
  }
  if (draft.corners.some(([x, y]) => Math.hypot(x - point[0], y - point[1]) < SAME_CORNER))
    return { kind: "drawing", draft }
  const corners = [...draft.corners, point]
  if (corners.length < 4) return { kind: "drawing", draft: { ...draft, corners } }
  const centre: Point = [
    corners.reduce((sum, [x]) => sum + x, 0) / 4,
    corners.reduce((sum, [, y]) => sum + y, 0) / 4,
  ]
  return { kind: "done", peerId, corners, centre }
}

/** The outline in progress on the stage; Escape abandons it. */
export function useOutlineDrawing() {
  const [draft, setDraft] = useState<OutlineDraft | null>(null)
  const draftRef = useRef(draft)
  draftRef.current = draft

  useEffect(() => {
    if (!draft) return
    function cancel(event: KeyboardEvent) {
      if (event.key !== "Escape") return
      event.preventDefault()
      event.stopPropagation()
      setDraft(null)
    }
    window.addEventListener("keydown", cancel, true)
    return () => window.removeEventListener("keydown", cancel, true)
  }, [draft])

  const place = useCallback((peerId: string, point: Point, aspect: number, shift: boolean) => {
    const step = placeCorner(draftRef.current, peerId, point, aspect, shift)
    draftRef.current = step.kind === "drawing" ? step.draft : null
    setDraft(draftRef.current)
    return step
  }, [])

  return { draft, place }
}

export type OutlineDrawing = ReturnType<typeof useOutlineDrawing>
