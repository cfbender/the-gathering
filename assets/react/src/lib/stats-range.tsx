import { createContext, useCallback, useContext, useMemo, useState } from "react"
import type { ReactNode } from "react"

/** How far back the stats pages look. Shared across pages and remembered per browser. */
export type StatsRange = "1m" | "6m" | "12m" | "all"

export const STATS_RANGES: readonly StatsRange[] = ["12m", "6m", "1m", "all"]
export const DEFAULT_STATS_RANGE: StatsRange = "12m"
const STORAGE_KEY = "the-gathering:stats-range"

const months: Record<Exclude<StatsRange, "all">, number> = { "1m": 1, "6m": 6, "12m": 12 }

export const statsRangeLabels: Record<StatsRange, string> = {
  "12m": "12 months",
  "6m": "6 months",
  "1m": "1 month",
  all: "All time",
}

/** Compact labels for narrow screens. */
export const statsRangeShortLabels: Record<StatsRange, string> = {
  "12m": "12 mo",
  "6m": "6 mo",
  "1m": "1 mo",
  all: "All",
}

/** Sentence-friendly description of the range, for stat card details. */
export const statsRangeDetails: Record<StatsRange, string> = {
  "12m": "last 12 months",
  "6m": "last 6 months",
  "1m": "last month",
  all: "all time",
}

export function isStatsRange(value: unknown): value is StatsRange {
  return typeof value === "string" && (STATS_RANGES as readonly string[]).includes(value)
}

/** Query params for the stats API: the inclusive local start date, or nothing for all time. */
export type StatsRangeParams = { date_from?: string }

function isoLocalDate(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${year}-${month}-${day}`
}

/**
 * The first day included in `range`, counted back from `today` in local time.
 * When the target month is shorter the date clamps to that month's last day,
 * so March 31 minus one month is February 28/29 rather than early March.
 */
export function statsRangeParams(range: StatsRange, today = new Date()): StatsRangeParams {
  if (range === "all") return {}
  const from = new Date(today.getFullYear(), today.getMonth() - months[range], 1)
  const lastDay = new Date(from.getFullYear(), from.getMonth() + 1, 0).getDate()
  from.setDate(Math.min(today.getDate(), lastDay))
  return { date_from: isoLocalDate(from) }
}

function storedRange(): StatsRange {
  try {
    const value = localStorage.getItem(STORAGE_KEY)
    return isStatsRange(value) ? value : DEFAULT_STATS_RANGE
  } catch {
    return DEFAULT_STATS_RANGE
  }
}

interface StatsRangeContextValue {
  range: StatsRange
  setRange: (range: StatsRange) => void
  /** Ready to spread into stats fetchers; stable for a given range and day. */
  params: StatsRangeParams
}

const StatsRangeContext = createContext<StatsRangeContextValue | null>(null)

export function StatsRangeProvider({ children }: { children: ReactNode }) {
  const [range, setRangeState] = useState<StatsRange>(storedRange)

  const setRange = useCallback((next: StatsRange) => {
    setRangeState(next)
    try {
      if (next === DEFAULT_STATS_RANGE) localStorage.removeItem(STORAGE_KEY)
      else localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // Storage may be unavailable; the in-memory range still applies.
    }
  }, [])

  const value = useMemo(
    () => ({ range, setRange, params: statsRangeParams(range) }),
    [range, setRange],
  )

  return <StatsRangeContext.Provider value={value}>{children}</StatsRangeContext.Provider>
}

export function useStatsRange(): StatsRangeContextValue {
  const context = useContext(StatsRangeContext)
  if (!context) throw new Error("useStatsRange must be used within StatsRangeProvider")
  return context
}
