import type { ReactNode } from "react"
import { cn } from "@/lib/cn"

export function StatCard({
  label,
  value,
  detail,
  icon,
  className,
}: {
  label: string
  value: ReactNode
  detail?: ReactNode
  icon?: ReactNode
  className?: string
}) {
  return (
    <div
      className={cn("border-base-300 bg-base-200/70 rounded-xl border p-4 shadow-sm", className)}
    >
      <div className="text-base-content/55 flex items-center justify-between text-xs font-bold tracking-wider uppercase">
        <span>{label}</span>
        {icon}
      </div>
      <div className="mt-2 text-3xl font-black tracking-tight tabular-nums">{value}</div>
      {detail && <div className="text-base-content/60 mt-1 text-xs">{detail}</div>}
    </div>
  )
}
