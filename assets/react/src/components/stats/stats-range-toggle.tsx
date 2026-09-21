import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import {
  STATS_RANGES,
  isStatsRange,
  statsRangeLabels,
  statsRangeShortLabels,
  useStatsRange,
  type StatsRange,
} from "@/lib/stats-range"

/** Segmented control for the shared stats time range (12 months, 6 months, 1 month, all time).
 * Phone widths show abbreviated labels so all four segments fit on one row. */
export function StatsRangeToggle({ className }: { className?: string }) {
  const { range, setRange } = useStatsRange()
  return (
    <ToggleGroup
      type="single"
      value={range}
      onValueChange={(value) => isStatsRange(value) && setRange(value)}
      aria-label="Stats time range"
      className={cn("join", className)}
    >
      {STATS_RANGES.map((value: StatsRange) => (
        <ToggleGroupItem
          key={value}
          value={value}
          aria-label={statsRangeLabels[value]}
          className={cn("btn btn-sm join-item", range === value ? "btn-primary" : "btn-ghost")}
        >
          <span className="sm:hidden">{statsRangeShortLabels[value]}</span>
          <span className="hidden sm:inline">{statsRangeLabels[value]}</span>
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
