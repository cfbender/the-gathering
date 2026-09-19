import type { ReactNode } from "react"

export function StatCard({
  label,
  value,
  detail,
  icon,
}: {
  label: string
  value: ReactNode
  detail?: ReactNode
  icon?: ReactNode
}) {
  return (
    <div className="border-base-300 bg-base-200/70 rounded-xl border p-4 shadow-sm">
      <div className="text-base-content/55 flex items-center justify-between text-xs font-bold tracking-wider uppercase">
        <span>{label}</span>
        {icon}
      </div>
      <div className="mt-2 text-3xl font-black tracking-tight tabular-nums">{value}</div>
      {detail && <div className="text-base-content/60 mt-1 text-xs">{detail}</div>}
    </div>
  )
}
