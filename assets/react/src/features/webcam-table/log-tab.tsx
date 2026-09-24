import { ScrollText } from "lucide-react"
import { PanelSection } from "./panel-section"
import type { TableEvent } from "./table-events"

export function LogTab({ events }: { events: TableEvent[] }) {
  return (
    <PanelSection title="Table log" icon={ScrollText}>
      {events.length === 0 ? (
        <p className="text-base-content/55 text-xs">Nothing has happened yet.</p>
      ) : (
        <ol className="grid gap-1 text-xs">
          {events.map((event) => (
            <li key={event.id} className="flex gap-2">
              <time
                className="text-base-content/45 shrink-0 tabular-nums"
                dateTime={event.at.toISOString()}
              >
                {event.at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
              </time>
              <span>
                {event.text}
                {event.count && <span className="ml-1 text-white/40">×{event.count}</span>}
              </span>
            </li>
          ))}
        </ol>
      )}
    </PanelSection>
  )
}
