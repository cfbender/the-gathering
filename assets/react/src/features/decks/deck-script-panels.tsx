import { AlertTriangle, ChevronDown, ChevronRight, Loader2, ScrollText } from "lucide-react"
import { Fragment, useState } from "react"
import { ColorIdentity, ManaSymbol } from "@/components/mana-symbols"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/cn"
import type {
  PreviewDeck,
  ScriptPreview,
  ScriptTable,
  ScriptValue,
} from "@/features/decks/deck-chooser"
import { DECK_SCRIPT_REFERENCE, inlineParts } from "@/features/decks/deck-script-reference"
import { LuaCode } from "@/features/decks/lua-highlight"

const weightFormat = new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 })
const percentFormat = new Intl.NumberFormat(undefined, {
  style: "percent",
  maximumFractionDigits: 1,
})

/** A script value written the way Lua would spell the table the script receives. */
export function toLua(value: ScriptValue, indent = ""): string {
  if (value === null) return "nil"
  if (typeof value === "string") return JSON.stringify(value)
  if (typeof value !== "object") return String(value)
  const inner = `${indent}  `
  if (Array.isArray(value)) {
    if (value.length === 0) return "{}"
    if (value.every((item) => item === null || typeof item !== "object")) {
      return `{ ${value.map((item) => toLua(item)).join(", ")} }`
    }
    return `{\n${value.map((item) => inner + toLua(item, inner)).join(",\n")}\n${indent}}`
  }
  const entries = Object.entries(value)
  if (entries.length === 0) return "{}"
  const fields = entries.map(([key, item]) => {
    const name = /^[A-Za-z_]\w*$/.test(key) ? key : `[${JSON.stringify(key)}]`
    return `${inner}${name} = ${toLua(item, inner)}`
  })
  return `{\n${fields.join(",\n")}\n${indent}}`
}

function LuaBlock({ value, label }: { value: ScriptValue; label: string }) {
  return (
    <pre
      aria-label={label}
      className="bg-base-100 border-base-300 max-h-96 overflow-auto rounded-box border p-3 font-mono text-xs leading-5"
    >
      <LuaCode code={toLua(value)} />
    </pre>
  )
}

function num(table: ScriptTable | null | undefined, key: string): number | null {
  const value = table?.[key]
  return typeof value === "number" ? value : null
}

function str(table: ScriptTable | null | undefined, key: string): string | null {
  const value = table?.[key]
  return typeof value === "string" ? value : null
}

function table(value: ScriptValue | undefined): ScriptTable | null {
  return value && typeof value === "object" && !Array.isArray(value) ? value : null
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1)
}

/** "3 h ago", "5 d ago", or "Never" from an hour count. */
export function formatHoursAgo(hours: number | null) {
  if (hours === null) return "Never"
  if (hours < 1) return "Just now"
  if (hours < 48) return `${Math.round(hours)} h ago`
  return `${Math.round(hours / 24)} d ago`
}

function formatLocalTime(time: ScriptTable | null) {
  const hour = num(time, "hour")
  const minute = num(time, "minute")
  const weekday = str(time, "weekday_name")
  if (hour === null || minute === null || !weekday) return "Unknown"
  const clock = new Date(2000, 0, 1, hour, minute).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  })
  return `${capitalize(weekday)} ${clock}`
}

function describeGame(game: ScriptTable | null) {
  if (!game) return "None yet"
  const result = str(game, "result")
  const verb = result === "win" ? "Won" : result === "loss" ? "Lost" : "Drew"
  const deck = str(game, "deck_name")
  const length = num(game, "duration_minutes")
  return [
    deck ? `${verb} with ${deck}` : verb,
    length !== null ? `${length} min` : null,
    formatHoursAgo(num(game, "hours_ago")),
  ]
    .filter(Boolean)
    .join(" · ")
}

function SummaryItem({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <div className="bg-base-100/70 border-base-300 min-w-0 rounded-box border px-3 py-2">
      <dt className="text-base-content/55 text-[0.65rem] font-bold tracking-wider uppercase">
        {label}
      </dt>
      <dd className="text-sm font-semibold break-words">{value}</dd>
      {detail && <dd className="text-base-content/60 truncate text-xs">{detail}</dd>}
    </div>
  )
}

/** The headline values of `ctx`: the time, today's and tonight's games, the last game, and
 * recent colors. */
export function ContextSummary({ context }: { context: ScriptTable }) {
  const stats = table(context.stats)
  const winRate = num(stats, "win_rate")
  const colors = table(context.recent_colors)
  return (
    <dl className="grid grid-cols-2 gap-2 sm:grid-cols-3" aria-label="Script context">
      <SummaryItem
        label="ctx.now"
        value={formatLocalTime(table(context.now))}
        detail={str(context, "timezone") ?? undefined}
      />
      <SummaryItem
        label="ctx.games_today"
        value={String(num(context, "games_today") ?? 0)}
        detail={`${num(context, "session_games") ?? 0} this session`}
      />
      <SummaryItem
        label="ctx.stats"
        value={`${num(stats, "wins") ?? 0}–${num(stats, "losses") ?? 0}–${num(stats, "draws") ?? 0}`}
        detail={winRate !== null ? `${percentFormat.format(winRate)} wins` : "No games logged yet"}
      />
      <SummaryItem label="ctx.last_game" value={describeGame(table(context.last_game))} />
      <div className="bg-base-100/70 border-base-300 col-span-2 min-w-0 rounded-box border px-3 py-2">
        <dt className="text-base-content/55 text-[0.65rem] font-bold tracking-wider uppercase">
          ctx.recent_colors <span className="normal-case">(last 10 games)</span>
        </dt>
        <dd className="mt-1 flex flex-wrap gap-3 text-sm font-semibold">
          {["W", "U", "B", "R", "G", "C"].map((color) => (
            <span key={color} className="flex items-center gap-1">
              <ManaSymbol symbol={color} className="mx-0" /> {num(colors, color) ?? 0}
            </span>
          ))}
        </dd>
      </div>
    </dl>
  )
}

function DeckRow({ deck, scripted }: { deck: PreviewDeck; scripted: boolean }) {
  const [open, setOpen] = useState(false)
  const input = deck.input
  const hours = num(input, "hours_since_played")
  const changed = scripted && Math.abs(deck.weight - deck.base_weight) > 1e-9
  return (
    <Fragment>
      <tr className="align-top">
        <td className="min-w-36">
          <button
            type="button"
            className="flex items-start gap-1.5 text-left font-semibold hover:underline"
            aria-expanded={open}
            onClick={() => setOpen((value) => !value)}
          >
            {open ? (
              <ChevronDown className="mt-0.5 size-4 shrink-0" />
            ) : (
              <ChevronRight className="mt-0.5 size-4 shrink-0" />
            )}
            <span>{deck.name}</span>
          </button>
          <ColorIdentity
            colors={str(input, "color_identity") || (input.colorless ? "C" : "")}
            className="mt-1 ml-5.5 text-xs"
          />
          {deck.note && (
            <p className="text-secondary mt-1 ml-5.5 text-xs font-medium">{deck.note}</p>
          )}
        </td>
        <td title={hours !== null ? `${weightFormat.format(hours)} hours` : undefined}>
          {formatHoursAgo(hours)}
        </td>
        <td className="text-right tabular-nums">{num(input, "plays") ?? 0}</td>
        <td className="text-right tabular-nums">{num(input, "skips") ?? 0}</td>
        <td className="text-right whitespace-nowrap tabular-nums">
          {num(input, "wins") ?? 0}–{num(input, "losses") ?? 0}–{num(input, "draws") ?? 0}
        </td>
        <td className="text-right tabular-nums">
          <span className={cn("font-semibold", changed && "text-primary")}>
            {weightFormat.format(deck.weight)}
          </span>
          {changed && (
            <span className="text-base-content/55 block text-xs" title="deck.base_weight">
              was {weightFormat.format(deck.base_weight)}
            </span>
          )}
        </td>
        <td className="min-w-24">
          <div className="flex items-center gap-2">
            <progress
              className="progress progress-primary h-2 w-10"
              value={deck.chance}
              max={1}
              aria-label={`${deck.name} chance`}
            />
            <span className="tabular-nums">{percentFormat.format(deck.chance)}</span>
          </div>
        </td>
      </tr>
      {open && (
        <tr>
          <td colSpan={7} className="bg-base-200/60">
            <p className="text-base-content/60 mb-2 text-xs">
              The <code>deck</code> table <code>weight</code> received for {deck.name}:
            </p>
            <LuaBlock value={input} label={`${deck.name} deck table`} />
          </td>
        </tr>
      )}
    </Fragment>
  )
}

/** The draft's outcome: whether it ran, each deck's inputs, weight, and chance, and its log. */
export function PreviewPanel({
  preview,
  updating,
  onGoToLine,
}: {
  preview: ScriptPreview
  updating: boolean
  onGoToLine: (line: number) => void
}) {
  if (preview.reason === "player_not_linked") {
    return (
      <div role="alert" className="alert">
        Your account needs a linked player before the chooser can weigh your decks. Ask an
        administrator to link it.
      </div>
    )
  }
  const decks = [...preview.decks].sort(
    (a, b) => b.chance - a.chance || a.name.localeCompare(b.name),
  )
  const scripted = preview.source === "script"
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2 text-sm" aria-live="polite">
        <span className={cn("badge", scripted ? "badge-primary" : "badge-ghost")}>
          {scripted ? "Script weights" : "Built-in weights"}
        </span>
        {updating && (
          <span className="text-base-content/60 flex items-center gap-1 text-xs">
            <Loader2 className="size-3 animate-spin" /> Updating…
          </span>
        )}
      </div>
      {preview.error && (
        <div role="alert" className="alert alert-warning items-start">
          <AlertTriangle className="size-5 shrink-0" />
          <div className="min-w-0">
            <p className="font-bold">The script failed, so picks would use the built-in weights.</p>
            <p className="font-mono text-xs break-words whitespace-pre-wrap">
              {preview.error.message}
            </p>
          </div>
          {preview.error.line !== null && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => onGoToLine(preview.error?.line ?? 1)}
            >
              Line {preview.error.line}
            </Button>
          )}
        </div>
      )}
      {preview.context && <ContextSummary context={preview.context} />}
      {decks.length === 0 ? (
        <div className="alert">
          No decks are available to pick. Add a deck to your player or include one in random picks.
        </div>
      ) : (
        <div className="border-base-300 overflow-x-auto rounded-box border">
          <table className="table-xs table">
            <thead>
              <tr>
                <th>Deck</th>
                <th>Last played</th>
                <th className="text-right">Plays</th>
                <th className="text-right">Skips</th>
                <th className="text-right">W–L–D</th>
                <th className="text-right">Weight</th>
                <th>Chance</th>
              </tr>
            </thead>
            <tbody>
              {decks.map((deck) => (
                <DeckRow key={deck.id} deck={deck} scripted={scripted} />
              ))}
            </tbody>
          </table>
        </div>
      )}
      <section aria-label="Script log">
        <h3 className="text-base-content/60 mb-1 flex items-center gap-1.5 text-xs font-bold tracking-wider uppercase">
          <ScrollText className="size-3.5" /> Log
        </h3>
        {preview.logs.length > 0 ? (
          <pre className="bg-base-100 border-base-300 max-h-48 overflow-auto rounded-box border p-3 font-mono text-xs leading-5">
            {preview.logs.join("\n")}
          </pre>
        ) : (
          <p className="text-base-content/55 text-xs">
            Nothing printed. Call <code>print(...)</code> in your script to inspect values here.
          </p>
        )}
      </section>
    </div>
  )
}

/** The whole `ctx` table, as the script sees it. */
export function ContextPanel({ preview }: { preview: ScriptPreview }) {
  if (!preview.context) return <p className="text-base-content/60 text-sm">No context yet.</p>
  return (
    <div className="flex flex-col gap-3">
      <ContextSummary context={preview.context} />
      <p className="text-base-content/60 text-xs">
        Every <code>weight</code> call receives this <code>ctx</code>, plus <code>ctx.decks</code>{" "}
        with each deck table (open a deck in the preview to see its table).
      </p>
      <LuaBlock value={preview.context} label="ctx table" />
    </div>
  )
}

function Inline({ text }: { text: string }) {
  return (
    <>
      {inlineParts(text).map((part, index) =>
        part.code ? (
          <code key={index} className="bg-base-200 rounded px-1 text-[0.85em]">
            {part.text}
          </code>
        ) : (
          <Fragment key={index}>{part.text}</Fragment>
        ),
      )}
    </>
  )
}

/** Every field a script can read, from `docs/deck-chooser-scripts.md`. */
export function ReferencePanel() {
  return (
    <div className="flex flex-col gap-3 text-sm">
      <p className="text-base-content/70">
        Define <code>weight(deck, ctx)</code> and return a number zero or above, optionally followed
        by a note. Fields marked “or <code>nil</code>” can be missing, so check them before doing
        arithmetic. Times are Unix seconds unless the field is a <code>time</code> table.
      </p>
      {DECK_SCRIPT_REFERENCE.map((section) => (
        <details
          key={section.name}
          className="border-base-300 bg-base-100 rounded-box border"
          open={section.name === "deck"}
        >
          <summary className="cursor-pointer px-3 py-2 font-mono font-bold">
            {section.name}
            <span className="text-base-content/50 ml-2 font-sans text-xs font-normal">
              {section.fields.length} fields
            </span>
          </summary>
          <div className="border-base-300 border-t px-3 py-2">
            {section.intro && (
              <p className="text-base-content/70 mb-2 text-xs">
                <Inline text={section.intro} />
              </p>
            )}
            <dl className="divide-base-300 divide-y">
              {section.fields.map((field) => (
                <div key={field.name} className="grid gap-0.5 py-1.5 sm:grid-cols-[14rem_1fr]">
                  <dt className="font-mono text-xs font-semibold break-all">{field.name}</dt>
                  <dd className="text-xs">
                    <span className="text-base-content/55 mr-1.5 italic">
                      <Inline text={field.type} />
                    </span>
                    <Inline text={field.description} />
                  </dd>
                </div>
              ))}
            </dl>
          </div>
        </details>
      ))}
    </div>
  )
}
