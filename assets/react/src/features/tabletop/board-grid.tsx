import type { CSSProperties, ReactNode } from "react"
import { cn } from "@/lib/cn"
import type { Layout, SeatArea } from "./tabletop"

/** Seat tints from the active palette, so seats without commander art still tell apart. */
export const SEAT_TINTS = [
  "var(--color-primary)",
  "var(--color-secondary)",
  "var(--color-accent)",
  "var(--color-info)",
  "var(--color-success)",
  "var(--color-warning)",
]

export function seatTint(index: number) {
  return SEAT_TINTS[index % SEAT_TINTS.length]!
}

export function gridStyle(layout: Layout): CSSProperties {
  return {
    gridTemplateRows: `repeat(${layout.rows}, minmax(0, 1fr))`,
    gridTemplateColumns: `repeat(${layout.cols}, minmax(0, 1fr))`,
  }
}

function areaStyle(area: SeatArea): CSSProperties {
  return {
    gridRow: `${area.row} / span ${area.rowSpan ?? 1}`,
    gridColumn: `${area.col} / span ${area.colSpan ?? 1}`,
  }
}

/**
 * A seat's grid cell whose contents turn to face the player sitting at that edge. Quarter
 * turns swap the inner box's width and height, and the inner box is itself a size container,
 * so `cqw`/`cqh` inside always mean the seat's own width and height as its player sees them.
 */
export function SeatCell({
  area,
  className,
  innerClassName,
  children,
  label,
}: {
  area: SeatArea
  className?: string
  innerClassName?: string
  children: ReactNode
  label?: string
}) {
  const sideways = area.rotation === 90 || area.rotation === 270
  return (
    <section
      aria-label={label}
      className={cn("relative min-h-0 min-w-0", className)}
      style={{ ...areaStyle(area), containerType: "size" }}
    >
      <div
        className={cn("absolute top-1/2 left-1/2", innerClassName)}
        style={{
          width: sideways ? "100cqh" : "100cqw",
          height: sideways ? "100cqw" : "100cqh",
          transform: `translate(-50%, -50%) rotate(${area.rotation}deg)`,
          containerType: "size",
        }}
      >
        {children}
      </div>
    </section>
  )
}

/** A miniature of a layout: one tile per seat, with a notch on the edge its player sits at. */
export function LayoutPreview({ layout, className }: { layout: Layout; className?: string }) {
  return (
    <div className={cn("grid aspect-[9/16] gap-1", className)} style={gridStyle(layout)}>
      {layout.seats.map((area, index) => (
        <SeatCell key={index} area={area}>
          <div
            className="rounded-field relative size-full"
            style={{ background: `color-mix(in oklch, ${seatTint(index)} 70%, transparent)` }}
          >
            <span className="absolute bottom-1 left-1/2 h-1 w-1/3 -translate-x-1/2 rounded-full bg-white/80" />
          </div>
        </SeatCell>
      ))}
    </div>
  )
}
