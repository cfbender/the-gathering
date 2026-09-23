import { act, renderHook } from "@testing-library/react"
import { beforeEach, expect, it } from "vite-plus/test"
import { clampRailWidth, useTablePreferences } from "./table-preferences"

beforeEach(() => localStorage.clear())

it.each([null, {}, { turnSound: true }, { turnSound: false }])(
  "defaults sound on but honors an explicit choice: %j",
  (saved) => {
    if (saved) localStorage.setItem("the-gathering:table-preferences:1", JSON.stringify(saved))
    expect(renderHook(() => useTablePreferences(1)).result.current.turnSound).toBe(
      saved?.turnSound !== false,
    )
  },
)

it("clamps each rail independently and rejects nonfinite widths", () => {
  expect(clampRailWidth("camera", 175)).toBe(176)
  expect(clampRailWidth("camera", 237)).toBe(237)
  expect(clampRailWidth("camera", 361)).toBe(360)
  expect(clampRailWidth("panel", 239)).toBe(240)
  expect(clampRailWidth("panel", 417)).toBe(417)
  expect(clampRailWidth("panel", 481)).toBe(480)
  expect(clampRailWidth("camera", NaN)).toBe(240)
  expect(clampRailWidth("panel", Infinity)).toBe(288)
})

it("persists preferences per player and resets widths without re-enabling shortcuts", () => {
  const first = renderHook(() => useTablePreferences(7))
  act(() => {
    first.result.current.setWidth("camera", 270)
    first.result.current.setWidth("panel", 410)
    first.result.current.setHotkeys(false)
  })
  first.unmount()
  const second = renderHook(() => useTablePreferences(7))
  expect(second.result.current).toMatchObject({ camera: 270, panel: 410, hotkeys: false })
  const other = renderHook(() => useTablePreferences(8))
  expect(other.result.current).toMatchObject({ camera: 240, panel: 288, hotkeys: true })
  act(() => second.result.current.resetWidths())
  expect(second.result.current).toMatchObject({ camera: 240, panel: 288, hotkeys: false })
})

it("recovers from malformed or out-of-bounds saved values", () => {
  localStorage.setItem("the-gathering:table-preferences:1", "broken")
  expect(renderHook(() => useTablePreferences(1)).result.current).toMatchObject({
    camera: 240,
    panel: 288,
    hotkeys: true,
  })
  localStorage.setItem(
    "the-gathering:table-preferences:2",
    JSON.stringify({ camera: -5, panel: 9999, hotkeys: "false" }),
  )
  expect(renderHook(() => useTablePreferences(2)).result.current).toMatchObject({
    camera: 176,
    panel: 480,
    hotkeys: true,
  })
})

it("persists view, camera, stats and sound choices without accepting malformed values", () => {
  localStorage.setItem(
    "the-gathering:table-preferences:4",
    JSON.stringify({ quality: "4k", deviceId: 7, followTurn: "true", stats: 1 }),
  )
  const first = renderHook(() => useTablePreferences(4))
  expect(first.result.current).toMatchObject({
    quality: "auto",
    deviceId: "",
    cameraEnabled: true,
    followTurn: false,
    stats: false,
    turnSound: true,
  })
  act(() =>
    first.result.current.update({
      quality: "720p",
      deviceId: "camera-2",
      cameraEnabled: false,
      followTurn: true,
      panelLeft: true,
      stats: true,
      turnSound: true,
    }),
  )
  first.unmount()
  const next = renderHook(() => useTablePreferences(4))
  expect(next.result.current).toMatchObject({
    quality: "720p",
    deviceId: "camera-2",
    cameraEnabled: false,
    followTurn: true,
    panelLeft: true,
    stats: true,
    turnSound: true,
  })
  act(() => next.result.current.resetWidths())
  expect(next.result.current).toMatchObject({
    quality: "720p",
    deviceId: "camera-2",
    cameraEnabled: false,
    followTurn: true,
    panelLeft: true,
    stats: true,
    turnSound: true,
  })
})
