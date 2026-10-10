import { Link } from "@tanstack/react-router"
import { Eraser, RefreshCw } from "lucide-react"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ApiError } from "@/lib/api"
import { cn } from "@/lib/cn"
import {
  MAX_LOG_ITEMS,
  useServerLogStream,
  type ServerLogConnection,
  type ServerLogEntry,
  type ServerLogItem,
  type ServerLogLevel,
} from "./server-log-stream"

const LEVEL_BADGE: Record<ServerLogLevel, string> = {
  debug: "badge-ghost",
  info: "badge-info",
  warning: "badge-warning",
  error: "badge-error",
}

const sudoRequired = new ApiError(403, "Reauthentication required", {
  code: "sudo_required",
  detail: "Reauthentication required",
})

function ConnectionBadge({ connection }: { connection: ServerLogConnection }) {
  const [label, tone] =
    connection.status === "live"
      ? ["Live", "badge-success"]
      : connection.status === "connecting"
        ? ["Connecting…", "badge-ghost"]
        : connection.status === "disconnected" && connection.retrying
          ? ["Disconnected — reconnecting…", "badge-warning"]
          : ["Disconnected", "badge-error"]
  return (
    <span role="status" className={cn("badge badge-outline gap-2", tone)}>
      {label}
    </span>
  )
}

function ConnectionNotice({
  connection,
  onReconnect,
}: {
  connection: ServerLogConnection
  onReconnect: () => void
}) {
  if (connection.status === "unauthorized" && connection.reason === "sudo_required") {
    return <SudoPrompt error={sudoRequired} onSuccess={onReconnect} />
  }
  if (connection.status === "unauthorized") {
    return (
      <div role="alert" className="alert alert-error">
        {connection.reason === "forbidden" ? (
          <span>Only administrators can read server logs.</span>
        ) : (
          <span>
            Your session ended.{" "}
            <Link
              to="/login"
              search={{ returnTo: "/admin/server-logs", error: undefined }}
              className="link"
            >
              Sign in again
            </Link>{" "}
            to keep following server logs.
          </span>
        )}
      </div>
    )
  }
  if (connection.status === "disconnected" && !connection.retrying) {
    return (
      <div role="alert" className="alert alert-warning">
        <span>The log stream closed.</span>
        <button type="button" className="btn btn-sm" onClick={onReconnect}>
          <RefreshCw className="size-4" aria-hidden="true" />
          Reconnect
        </button>
      </div>
    )
  }
  return null
}

function formatTime(timestamp: string) {
  const date = new Date(timestamp)
  return Number.isNaN(date.getTime()) ? timestamp : date.toLocaleTimeString()
}

function EntryRow({ entry }: { entry: ServerLogEntry }) {
  const fields = Object.entries(entry.fields)
  return (
    <li className="border-base-300 flex flex-col gap-1 border-b px-4 py-2 last:border-b-0">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <time dateTime={entry.timestamp} className="text-base-content/70 font-mono">
          {formatTime(entry.timestamp)}
        </time>
        <span className={cn("badge badge-sm uppercase", LEVEL_BADGE[entry.level])}>
          {entry.level}
        </span>
        <span className="text-base-content/60 font-mono">{entry.target}</span>
      </div>
      <p className="font-mono text-sm break-words whitespace-pre-wrap">{entry.message}</p>
      {(entry.request || fields.length > 0) && (
        <p className="text-base-content/60 font-mono text-xs break-all">
          {entry.request &&
            `${entry.request.method} ${entry.request.path} · ${entry.request.request_id}`}
          {entry.request && fields.length > 0 && " · "}
          {fields.map(([name, value]) => `${name}=${value}`).join(" ")}
        </p>
      )}
    </li>
  )
}

function LogItem({ item }: { item: ServerLogItem }) {
  if (item.kind === "entry") return <EntryRow entry={item.entry} />
  return (
    <li className="text-warning border-base-300 border-b px-4 py-2 text-sm last:border-b-0">
      {item.dropped} {item.dropped === 1 ? "message was" : "messages were"} dropped while the stream
      caught up.
    </li>
  )
}

export function ServerLogsPage() {
  const { items, connection, reconnect, clear } = useServerLogStream()

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Server logs"
        description={`Live messages from this server while this page is open, newest first. Nothing is stored; only the newest ${MAX_LOG_ITEMS} stay here.`}
        actions={
          <>
            <ConnectionBadge connection={connection} />
            <button
              type="button"
              className="btn btn-sm"
              onClick={clear}
              disabled={items.length === 0}
            >
              <Eraser className="size-4" aria-hidden="true" />
              Clear
            </button>
          </>
        }
      />
      <ConnectionNotice connection={connection} onReconnect={reconnect} />
      {items.length === 0 ? (
        <p className="text-base-content/70 text-sm">
          {connection.status === "live" ? "Waiting for log messages…" : "No log messages yet."}
        </p>
      ) : (
        <ol
          aria-label="Server log messages"
          className="card border-base-300 bg-base-200 overflow-hidden border"
        >
          {items.map((item) => (
            <LogItem key={item.key} item={item} />
          ))}
        </ol>
      )}
    </div>
  )
}
