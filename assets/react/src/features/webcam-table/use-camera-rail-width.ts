import { useLayoutEffect, useRef, useState, type RefObject } from "react"
import { RAIL_WIDTHS } from "./table-preferences"

/** Boards without a playing video (placeholders, connecting peers) assume a 16:9 camera. */
const FALLBACK_ASPECT = 16 / 9
/** The rail may always reach its original limit, even when the board is already height-bound. */
const BASE_MAX = { px: 360, viewportShare: 0.24 }

export interface StageBoardBox {
  width: number
  height: number
  /** The video's intrinsic width / height. */
  aspect: number
}

/** The widest camera rail that still leaves every stage board at least as wide as its
 * `object-contain` video at full height. Up to this width the rail only eats the board's
 * pillarbox bars; past it the main camera would start shrinking. The original
 * `min(360px, 24vw)` limit stays available when the board is already height-bound. */
export function cameraRailFitWidth(
  railWidth: number,
  boards: StageBoardBox[],
  viewportWidth: number,
): number | null {
  if (boards.length === 0) return null
  const slack = Math.min(...boards.map(({ width, height, aspect }) => width - height * aspect))
  const base = Math.min(BASE_MAX.px, viewportWidth * BASE_MAX.viewportShare)
  return Math.floor(Math.max(base, railWidth + slack))
}

function measureBoards(stage: HTMLElement): StageBoardBox[] {
  return Array.from(stage.querySelectorAll<HTMLElement>("[data-stage-board]"), (board) => {
    const { width, height } = board.getBoundingClientRect()
    const video = board.querySelector("video")
    const aspect =
      video && video.videoWidth && video.videoHeight
        ? video.videoWidth / video.videoHeight
        : FALLBACK_ASPECT
    return { width, height, aspect }
  })
}

/** The camera rail's shown width and drag limit: the saved width, capped by
 * `cameraRailFitWidth` as the window, the active board, or a video's resolution changes.
 * The fit is measured against the width the rail had in that layout, so it holds still while
 * the rail resizes within it. `boardsKey` must change whenever the stage swaps its boards. */
export function useCameraRailWidth(
  stageRef: RefObject<HTMLElement | null>,
  savedWidth: number,
  boardsKey: string,
): { width: number; max: number } {
  const [fit, setFit] = useState<number | null>(null)
  const max = Math.max(RAIL_WIDTHS.camera.min, Math.min(RAIL_WIDTHS.camera.max, fit ?? savedWidth))
  const width = Math.min(savedWidth, max)
  const railWidthRef = useRef(width)
  useLayoutEffect(() => {
    railWidthRef.current = width
  }, [width])

  useLayoutEffect(() => {
    const stage = stageRef.current
    if (!stage) return
    const measure = () => {
      const next = cameraRailFitWidth(railWidthRef.current, measureBoards(stage), window.innerWidth)
      setFit((current) => (current === next ? current : next))
    }
    measure()
    // Media `resize`/`loadedmetadata` do not bubble, so listen in the capture phase.
    stage.addEventListener("loadedmetadata", measure, true)
    stage.addEventListener("resize", measure, true)
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure)
    observer?.observe(stage)
    stage.querySelectorAll("[data-stage-board]").forEach((board) => observer?.observe(board))
    return () => {
      stage.removeEventListener("loadedmetadata", measure, true)
      stage.removeEventListener("resize", measure, true)
      observer?.disconnect()
    }
  }, [stageRef, boardsKey])

  return { width, max }
}
