import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vite-plus/test"
import { StatsRangeToggle } from "@/components/stats/stats-range-toggle"
import { StatsRangeProvider, statsRangeParams, useStatsRange } from "@/lib/stats-range"

describe("statsRangeParams", () => {
  it("returns no filter for all time", () => {
    expect(statsRangeParams("all", new Date(2026, 8, 21))).toEqual({})
  })

  it("counts whole months back from today in local time", () => {
    const today = new Date(2026, 8, 21) // 21 Sep 2026
    expect(statsRangeParams("1m", today)).toEqual({ date_from: "2026-08-21" })
    expect(statsRangeParams("6m", today)).toEqual({ date_from: "2026-03-21" })
    expect(statsRangeParams("12m", today)).toEqual({ date_from: "2025-09-21" })
  })

  it("wraps across the year boundary", () => {
    expect(statsRangeParams("6m", new Date(2026, 1, 10))).toEqual({ date_from: "2025-08-10" })
  })

  it("clamps to the last day of a shorter target month", () => {
    expect(statsRangeParams("1m", new Date(2026, 2, 31))).toEqual({ date_from: "2026-02-28" })
    expect(statsRangeParams("12m", new Date(2028, 1, 29))).toEqual({ date_from: "2027-02-28" })
  })
})

function CurrentRange() {
  const { range, params } = useStatsRange()
  return (
    <output data-testid="range">
      {range}:{params.date_from ?? "none"}
    </output>
  )
}

describe("StatsRangeToggle", () => {
  afterEach(() => {
    cleanup()
    localStorage.clear()
  })

  it("defaults to 12 months and switches the shared range", () => {
    render(
      <StatsRangeProvider>
        <StatsRangeToggle />
        <CurrentRange />
      </StatsRangeProvider>,
    )
    expect(screen.getByRole("radio", { name: "12 months" }).getAttribute("aria-checked")).toBe(
      "true",
    )
    expect(screen.getByTestId("range").textContent).toMatch(/^12m:\d{4}-\d{2}-\d{2}$/)

    fireEvent.click(screen.getByRole("radio", { name: "All time" }))
    expect(screen.getByTestId("range").textContent).toBe("all:none")
    expect(localStorage.getItem("the-gathering:stats-range")).toBe("all")
  })

  it("keeps a selection when the pressed segment is clicked again", () => {
    render(
      <StatsRangeProvider>
        <StatsRangeToggle />
        <CurrentRange />
      </StatsRangeProvider>,
    )
    fireEvent.click(screen.getByRole("radio", { name: "1 month" }))
    fireEvent.click(screen.getByRole("radio", { name: "1 month" }))
    expect(screen.getByTestId("range").textContent).toMatch(/^1m:/)
  })

  it("restores the remembered range", () => {
    localStorage.setItem("the-gathering:stats-range", "6m")
    render(
      <StatsRangeProvider>
        <CurrentRange />
      </StatsRangeProvider>,
    )
    expect(screen.getByTestId("range").textContent).toMatch(/^6m:/)
  })
})
