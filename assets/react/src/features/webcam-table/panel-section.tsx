import { ChevronDown } from "lucide-react"
import { useState, type ComponentType, type ReactNode } from "react"
import { cn } from "@/lib/cn"

/** One collapsible, titled section of the side panel. */
export function PanelSection({
  title,
  icon: Icon,
  meta,
  defaultOpen = true,
  children,
}: {
  title: string
  icon: ComponentType<{ className?: string }>
  meta?: ReactNode
  defaultOpen?: boolean
  children: ReactNode
}) {
  const [expanded, setExpanded] = useState(defaultOpen)
  return (
    <section className="border-b border-white/10">
      <button
        type="button"
        className="hover:bg-white/5 flex h-9 w-full items-center gap-2 px-3 text-left text-xs font-bold"
        onClick={() => setExpanded((value) => !value)}
        aria-expanded={expanded}
      >
        <Icon className="text-base-content/60 size-3.5" />
        <span className="flex-1">{title}</span>
        {meta}
        <ChevronDown
          className={cn("text-base-content/60 size-3.5 transition", !expanded && "-rotate-90")}
        />
      </button>
      {expanded && <div className="px-3 pb-3">{children}</div>}
    </section>
  )
}
