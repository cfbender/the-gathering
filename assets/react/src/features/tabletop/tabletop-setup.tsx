import { useNavigate } from "@tanstack/react-router"
import { ArrowRight, Heart, Minus, Plus, RotateCcw } from "lucide-react"
import type { ReactNode } from "react"
import { PageHeader } from "@/components/app-shell"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { isGameFormat } from "@/features/games/game-format"
import { cn } from "@/lib/cn"
import { LayoutPreview } from "./board-grid"
import {
  TABLETOP_FORMATS,
  eliminationReason,
  layoutsFor,
  newGame,
  seatCounts,
  seatName,
  startingLife,
  type TabletopFormat,
  type TabletopSettings,
} from "./tabletop"
import { useTabletopGame, useTabletopSettings } from "./tabletop-store"

const LIFE_PRESETS = [20, 30, 40]

/** Picks a format, the number of players, how they sit around the device, and starting life,
 * then hands over to the board, where everyone claims a seat. */
export function TabletopSetup() {
  const navigate = useNavigate()
  const [game, setGame] = useTabletopGame()
  const [settings, setSettings] = useTabletopSettings()
  const counts = seatCounts(settings.format)
  const layouts = layoutsFor(settings.seatCount)

  const update = (next: Partial<TabletopSettings>) => setSettings({ ...settings, ...next })

  const chooseFormat = (format: TabletopFormat) => {
    const allowed = seatCounts(format)
    const seatCount = allowed.includes(settings.seatCount) ? settings.seatCount : allowed[0]!
    update({
      format,
      seatCount,
      layoutId: layoutsFor(seatCount).some((layout) => layout.id === settings.layoutId)
        ? settings.layoutId
        : layoutsFor(seatCount)[0]!.id,
      startingLife: startingLife(format),
    })
  }

  const chooseCount = (seatCount: number) => {
    const options = layoutsFor(seatCount)
    update({
      seatCount,
      layoutId: options.some((layout) => layout.id === settings.layoutId)
        ? settings.layoutId
        : options[0]!.id,
    })
  }

  const chooseSeats = () => {
    setGame(newGame(settings, game ?? undefined))
    void navigate({ to: "/tabletop/play" })
  }

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6">
      <PageHeader
        eyebrow="One device, whole table"
        title="Tabletop"
        description="Lay a phone or tablet in the middle of the table to track life, commander damage, and turns. When everyone at the table is in the playgroup, the result goes straight into game history."
      />

      {game && (
        <section className="card border-primary/40 bg-base-200 border">
          <div className="card-body gap-4 p-5 sm:p-6">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2 className="text-xl font-black">
                  {game.phase === "playing" ? "Game in progress" : "Table waiting for players"}
                </h2>
                <p className="text-base-content/65 text-sm">
                  Saved on this device. Pick up where you left off.
                </p>
              </div>
              <div className="flex gap-2">
                <button type="button" className="btn btn-ghost" onClick={() => setGame(null)}>
                  Discard
                </button>
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={() => void navigate({ to: "/tabletop/play" })}
                >
                  <RotateCcw className="size-4" /> Resume
                </button>
              </div>
            </div>
            <ul className="flex flex-wrap gap-2">
              {game.seats.map((seat, index) => (
                <li
                  key={index}
                  className={cn(
                    "badge badge-lg gap-2",
                    eliminationReason(seat, game.format) && game.phase === "playing"
                      ? "badge-ghost line-through opacity-60"
                      : "badge-outline",
                  )}
                >
                  {seatName(seat, index)}
                  {game.phase === "playing" && (
                    <span className="font-black tabular-nums">{seat.life}</span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        </section>
      )}

      <form
        className="card border-base-300 bg-base-100 border"
        onSubmit={(event) => {
          event.preventDefault()
          chooseSeats()
        }}
      >
        <div className="card-body gap-7 p-5 sm:p-6">
          <Step title="Format">
            <ToggleGroup
              type="single"
              value={settings.format}
              onValueChange={(value) => value && chooseFormat(value as TabletopFormat)}
              aria-label="Format"
              className="grid grid-cols-2 gap-2 sm:grid-cols-4"
            >
              {TABLETOP_FORMATS.map((format) => (
                <ToggleGroupItem
                  key={format.value}
                  value={format.value}
                  className={cn(
                    "btn h-auto min-h-14 flex-col gap-0.5 px-3 py-2",
                    settings.format === format.value ? "btn-primary" : "btn-outline",
                  )}
                >
                  {format.label}
                  {!isGameFormat(format.value) && (
                    <span className="text-[0.65rem] font-normal opacity-70">Not recorded</span>
                  )}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </Step>

          <Step title="Players">
            <ToggleGroup
              type="single"
              value={String(settings.seatCount)}
              onValueChange={(value) => value && chooseCount(Number(value))}
              aria-label="Players"
              className="grid grid-cols-5 gap-2"
            >
              {[2, 3, 4, 5, 6].map((count) => (
                <ToggleGroupItem
                  key={count}
                  value={String(count)}
                  disabled={!counts.includes(count)}
                  className={cn(
                    "btn btn-lg text-xl font-black tabular-nums",
                    settings.seatCount === count ? "btn-primary" : "btn-outline",
                  )}
                >
                  {count}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </Step>

          <Step title="Seating" hint="Each bar marks the edge of the device that player sits at.">
            <ToggleGroup
              type="single"
              value={settings.layoutId}
              onValueChange={(value) => value && update({ layoutId: value })}
              aria-label="Seating"
              className="flex flex-wrap gap-3"
            >
              {layouts.map((layout) => (
                <ToggleGroupItem
                  key={layout.id}
                  value={layout.id}
                  className={cn(
                    "rounded-box flex w-32 flex-col items-center gap-2 border-2 p-2.5 text-sm font-bold transition-colors",
                    settings.layoutId === layout.id
                      ? "border-primary bg-primary/10"
                      : "border-base-300 hover:border-base-content/30",
                  )}
                >
                  <LayoutPreview layout={layout} className="w-full" />
                  {layout.label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </Step>

          <Step title="Starting life">
            <div className="flex flex-wrap items-center gap-3">
              <div className="join">
                <button
                  type="button"
                  className="btn btn-square join-item"
                  aria-label="Less starting life"
                  disabled={settings.startingLife <= 1}
                  onClick={() => update({ startingLife: Math.max(1, settings.startingLife - 1) })}
                >
                  <Minus className="size-4" />
                </button>
                <span className="join-item bg-base-200 flex min-w-18 items-center justify-center gap-1.5 px-3 text-xl font-black tabular-nums">
                  <Heart className="text-primary size-4" fill="currentColor" />
                  {settings.startingLife}
                </span>
                <button
                  type="button"
                  className="btn btn-square join-item"
                  aria-label="More starting life"
                  disabled={settings.startingLife >= 999}
                  onClick={() => update({ startingLife: Math.min(999, settings.startingLife + 1) })}
                >
                  <Plus className="size-4" />
                </button>
              </div>
              <div className="flex gap-1">
                {LIFE_PRESETS.map((life) => (
                  <button
                    key={life}
                    type="button"
                    className={cn(
                      "btn btn-sm",
                      settings.startingLife === life ? "btn-neutral" : "btn-ghost",
                    )}
                    onClick={() => update({ startingLife: life })}
                  >
                    {life}
                  </button>
                ))}
              </div>
            </div>
          </Step>

          <div className="flex justify-end">
            <button className="btn btn-primary btn-lg">
              Choose seats <ArrowRight className="size-5" />
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}

function Step({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <fieldset className="grid gap-3">
      <legend className="mb-3 text-lg font-black">{title}</legend>
      {children}
      {hint && <p className="text-base-content/60 -mt-1 text-xs">{hint}</p>}
    </fieldset>
  )
}
