import {
  ChevronLeft,
  ChevronRight,
  Gamepad2,
  Layers,
  ScrollText,
  Settings,
  WalletCards,
} from "lucide-react"
import type { ComponentType, ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/cn"
import { CardsTab, type CardsTabProps } from "./cards-tab"
import { DecksTab } from "./decks-tab"
import { LogTab } from "./log-tab"
import type { TableEvent } from "./table-events"
import { TableTab, type TableTabProps } from "./table-tab"

export type PanelTab = "table" | "decks" | "cards" | "log" | "settings"

interface Props extends CardsTabProps, TableTabProps {
  left?: boolean
  settings: ReactNode
  onHelp: () => void
  open: boolean
  tab: PanelTab
  onOpenChange: (open: boolean) => void
  onTabChange: (tab: PanelTab) => void
  events: TableEvent[]
}

const TABS: { id: PanelTab; label: string; icon: ComponentType<{ className?: string }> }[] = [
  { id: "table", label: "Table", icon: Gamepad2 },
  { id: "decks", label: "Decks", icon: Layers },
  { id: "cards", label: "Cards", icon: WalletCards },
  { id: "log", label: "Log", icon: ScrollText },
  { id: "settings", label: "Settings", icon: Settings },
]

function PanelContent(props: Props) {
  switch (props.tab) {
    case "table":
      return <TableTab {...props} />
    case "decks":
      return <DecksTab {...props} />
    case "cards":
      return <CardsTab {...props} />
    case "log":
      return <LogTab events={props.events} />
    case "settings":
      return props.settings
  }
}

/** Right-hand control column: a narrow icon strip that switches tabs and collapses the panel,
 * plus the stacked, collapsible sections for the active tab. */
export function SidePanel(props: Props) {
  const { open, tab, onOpenChange, onTabChange } = props

  return (
    <div
      className={cn(
        "bg-base-100 text-base-content flex max-h-[45dvh] flex-col border-t border-white/10 lg:max-h-none lg:flex-row lg:border-t-0 lg:border-l",
        props.left && "lg:order-1 lg:flex-row-reverse",
      )}
    >
      <nav
        className="flex shrink-0 items-center gap-1 px-1.5 py-1 lg:w-14 lg:flex-col lg:items-stretch lg:px-1 lg:py-1.5"
        aria-label="Table panels"
      >
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="text-base-content/60 hover:text-base-content h-7 min-h-7"
          onClick={() => onOpenChange(!open)}
          aria-label={open ? "Collapse panel" : "Expand panel"}
          aria-expanded={open}
        >
          {open ? (
            <ChevronRight className="size-4 -rotate-90 lg:rotate-0" />
          ) : (
            <ChevronLeft className="size-4 -rotate-90 lg:rotate-0" />
          )}
        </Button>
        {TABS.filter(({ id }) => !props.spectating || (id !== "decks" && id !== "settings")).map(
          ({ id, label, icon: Icon }) => {
            const active = open && tab === id
            return (
              <Button
                key={id}
                type="button"
                variant="ghost"
                size="sm"
                className={cn(
                  "h-auto flex-col gap-0.5 px-2 py-1.5 text-[0.55rem] font-semibold",
                  active
                    ? "bg-primary/20 text-primary"
                    : "text-base-content/60 hover:text-base-content",
                )}
                onClick={() => {
                  onTabChange(id)
                  onOpenChange(true)
                }}
                aria-pressed={active}
              >
                <Icon className="size-4" />
                {label}
              </Button>
            )
          },
        )}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={props.onHelp}
          aria-label="Keyboard shortcuts"
          title="Keyboard shortcuts (?)"
        >
          ?
        </Button>
      </nav>
      {open && (
        <div className="bg-base-200 min-h-0 min-w-0 flex-1 overflow-y-auto border-t border-white/10 lg:w-[var(--table-panel-width)] lg:flex-none lg:border-t-0 lg:border-l">
          {props.error && (
            <div
              role="alert"
              className="bg-error/20 text-error border-b border-white/10 px-3 py-2 text-xs"
            >
              {props.error}
            </div>
          )}
          <PanelContent {...props} />
        </div>
      )}
    </div>
  )
}
