import { Minus, Plus, Trash2 } from "lucide-react"
import { useMemo, useState } from "react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/cn"
import {
  COUNTER_MAX,
  MAX_COUNTERS,
  MAX_EFFECTS,
  PRESET_CONDITIONS,
  PT_LIMIT,
  clampBuff,
  clampCounter,
  combatRows,
  conditionKey,
  formatBuff,
  isPresetCondition,
  newCounter,
  newEffect,
  parseList,
  type CombatEffect,
  type CustomCounter,
  type SeatTrackers,
} from "./trackers"

const FIELD = "input input-sm input-bordered bg-base-100 focus:border-primary focus:outline-none"
const ICON_BUTTON = "btn btn-ghost btn-sm btn-square shrink-0"

function CounterEditor({
  counter,
  onChange,
  onRemove,
}: {
  counter: CustomCounter
  onChange: (counter: CustomCounter) => void
  onRemove: () => void
}) {
  const name = counter.label || "counter"
  return (
    <li className="flex flex-wrap items-center gap-2 py-1.5">
      <input
        className={cn(FIELD, "min-w-0 flex-1 basis-32")}
        value={counter.label}
        maxLength={40}
        placeholder="What are you counting?"
        aria-label="Counter name"
        onChange={(event) => onChange({ ...counter, label: event.target.value })}
      />
      <div className="flex items-center">
        <button
          type="button"
          className={ICON_BUTTON}
          aria-label={`Decrease ${name}`}
          disabled={counter.value === 0}
          onClick={() => onChange({ ...counter, value: clampCounter(counter.value - 1) })}
        >
          <Minus className="size-4" />
        </button>
        <input
          type="number"
          inputMode="numeric"
          min={0}
          max={COUNTER_MAX}
          className={cn(FIELD, "w-16 text-center font-bold tabular-nums")}
          value={counter.value}
          aria-label={`${name} value`}
          onChange={(event) =>
            onChange({ ...counter, value: clampCounter(event.target.valueAsNumber) })
          }
        />
        <button
          type="button"
          className={ICON_BUTTON}
          aria-label={`Increase ${name}`}
          disabled={counter.value === COUNTER_MAX}
          onClick={() => onChange({ ...counter, value: clampCounter(counter.value + 1) })}
        >
          <Plus className="size-4" />
        </button>
      </div>
      <label className="flex items-center gap-2 text-xs">
        <Switch
          size="sm"
          checked={counter.shared}
          onCheckedChange={(shared) => onChange({ ...counter, shared })}
          aria-label={`Show ${name} to the table`}
        />
        Show to table
      </label>
      <button
        type="button"
        className={cn(ICON_BUTTON, "text-error")}
        aria-label={`Remove ${name}`}
        onClick={onRemove}
      >
        <Trash2 className="size-4" />
      </button>
    </li>
  )
}

function BuffInput({
  label,
  value,
  onChange,
}: {
  label: string
  value: number
  onChange: (value: number) => void
}) {
  return (
    <input
      type="number"
      inputMode="numeric"
      min={-PT_LIMIT}
      max={PT_LIMIT}
      className={cn(FIELD, "w-16 text-center font-bold tabular-nums")}
      value={value}
      aria-label={label}
      onChange={(event) => onChange(clampBuff(event.target.valueAsNumber))}
    />
  )
}

/** A typed, comma-separated list. The draft is kept as typed so a trailing comma survives;
 * the parsed items are what the effect stores. */
function ListInput({
  items,
  label,
  placeholder,
  className,
  onChange,
}: {
  items: string[]
  label: string
  placeholder: string
  className?: string
  onChange: (items: string[]) => void
}) {
  const [draft, setDraft] = useState(items.join(", "))
  return (
    <input
      className={cn(FIELD, className)}
      value={draft}
      placeholder={placeholder}
      aria-label={label}
      onChange={(event) => {
        setDraft(event.target.value)
        onChange(parseList(event.target.value))
      }}
    />
  )
}

function EffectEditor({
  effect,
  onChange,
  onRemove,
}: {
  effect: CombatEffect
  onChange: (effect: CombatEffect) => void
  onRemove: () => void
}) {
  const name = effect.name || "buff"
  const presets = effect.conditions.filter(isPresetCondition)
  const custom = effect.conditions.filter((condition) => !isPresetCondition(condition))
  const toggle = (id: string, on: boolean) =>
    onChange({
      ...effect,
      conditions: on
        ? [...effect.conditions, id]
        : effect.conditions.filter((condition) => conditionKey(condition) !== id),
    })
  return (
    <li className="rounded-box border border-base-300 p-3">
      <div className="flex flex-wrap items-center gap-2">
        <input
          className={cn(FIELD, "min-w-0 flex-1 basis-40")}
          value={effect.name}
          maxLength={80}
          placeholder="Card or effect (e.g. Intangible Virtue)"
          aria-label="Buff name"
          onChange={(event) => onChange({ ...effect, name: event.target.value })}
        />
        <div className="flex items-center gap-1 text-sm font-bold">
          <BuffInput
            label={`${name} power`}
            value={effect.power}
            onChange={(power) => onChange({ ...effect, power })}
          />
          /
          <BuffInput
            label={`${name} toughness`}
            value={effect.toughness}
            onChange={(toughness) => onChange({ ...effect, toughness })}
          />
        </div>
        <button
          type="button"
          className={cn(ICON_BUTTON, "text-error")}
          aria-label={`Remove ${name}`}
          onClick={onRemove}
        >
          <Trash2 className="size-4" />
        </button>
      </div>
      <div
        className="mt-2 flex flex-wrap items-center gap-1.5"
        role="group"
        aria-label="Applies to"
      >
        <span className="mr-1 text-xs text-base-content/65">Applies to</span>
        {PRESET_CONDITIONS.map((preset) => {
          const on = presets.some((condition) => conditionKey(condition) === preset.id)
          return (
            <button
              key={preset.id}
              type="button"
              className={cn("btn btn-xs", on ? "btn-primary" : "btn-ghost border-base-300")}
              aria-pressed={on}
              onClick={() => toggle(preset.id, !on)}
            >
              {preset.label}
            </button>
          )
        })}
        <ListInput
          items={custom}
          label={`${name} other conditions`}
          placeholder="Other (Elf, Goblin…)"
          className="input-xs min-w-0 flex-1 basis-32"
          onChange={(items) => onChange({ ...effect, conditions: [...presets, ...items] })}
        />
      </div>
      <ListInput
        items={effect.keywords}
        label={`${name} keywords`}
        placeholder="Keywords granted (vigilance, trample…)"
        className="mt-2 w-full"
        onChange={(keywords) => onChange({ ...effect, keywords })}
      />
    </li>
  )
}

/** Edits this seat's custom counters and combat buffs; every change applies immediately. */
export function TrackersDialog({
  open,
  onOpenChange,
  trackers,
  onChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  trackers: SeatTrackers
  onChange: (update: (current: SeatTrackers) => SeatTrackers) => void
}) {
  const rows = useMemo(() => combatRows(trackers.effects), [trackers.effects])
  const replace = <T extends { id: string }>(items: T[], next: T) =>
    items.map((item) => (item.id === next.id ? next : item))
  const remove = <T extends { id: string }>(items: T[], id: string) =>
    items.filter((item) => item.id !== id)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl" aria-label="Trackers">
        <DialogHeader>
          <div>
            <DialogTitle>Trackers</DialogTitle>
            <p className="mt-0.5 text-sm text-base-content/65">
              Counters and combat math over your camera. Share what the table should see.
            </p>
          </div>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-5 py-4">
          <section aria-labelledby="trackers-counters">
            <h3 id="trackers-counters" className="text-sm font-bold">
              Counters
            </h3>
            <p className="mt-0.5 text-xs text-base-content/65">
              Lands, creatures in the graveyard, storm count: anything from 0 to {COUNTER_MAX}.
            </p>
            {trackers.counters.length > 0 && (
              <ul className="mt-2 divide-y divide-base-300">
                {trackers.counters.map((counter) => (
                  <CounterEditor
                    key={counter.id}
                    counter={counter}
                    onChange={(next) =>
                      onChange((current) => ({
                        ...current,
                        counters: replace(current.counters, next),
                      }))
                    }
                    onRemove={() =>
                      onChange((current) => ({
                        ...current,
                        counters: remove(current.counters, counter.id),
                      }))
                    }
                  />
                ))}
              </ul>
            )}
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="mt-2"
              disabled={trackers.counters.length >= MAX_COUNTERS}
              onClick={() =>
                onChange((current) => ({
                  ...current,
                  counters: [...current.counters, newCounter(current.counters.length + 1)],
                }))
              }
            >
              <Plus className="size-3.5" /> Add counter
            </Button>
          </section>

          <section aria-labelledby="trackers-combat">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h3 id="trackers-combat" className="text-sm font-bold">
                Combat buffs
              </h3>
              <label className="flex items-center gap-2 text-xs">
                <Switch
                  size="sm"
                  checked={trackers.shareEffects}
                  onCheckedChange={(shareEffects) =>
                    onChange((current) => ({ ...current, shareEffects }))
                  }
                  aria-label="Show combat buffs to the table"
                />
                Show to table
              </label>
            </div>
            <p className="mt-0.5 text-xs text-base-content/65">
              Add each anthem and combat trigger on your board; the totals combine them for every
              kind of creature.
            </p>
            {trackers.effects.length > 0 && (
              <ul className="mt-2 space-y-2">
                {trackers.effects.map((effect) => (
                  <EffectEditor
                    key={effect.id}
                    effect={effect}
                    onChange={(next) =>
                      onChange((current) => ({
                        ...current,
                        effects: replace(current.effects, next),
                      }))
                    }
                    onRemove={() =>
                      onChange((current) => ({
                        ...current,
                        effects: remove(current.effects, effect.id),
                      }))
                    }
                  />
                ))}
              </ul>
            )}
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="mt-2"
              disabled={trackers.effects.length >= MAX_EFFECTS}
              onClick={() =>
                onChange((current) => ({ ...current, effects: [...current.effects, newEffect()] }))
              }
            >
              <Plus className="size-3.5" /> Add buff
            </Button>
            {rows.length > 0 && (
              <ul
                className="mt-3 divide-y divide-base-300 rounded-box border border-base-300 text-sm"
                aria-label="Combat totals"
              >
                {rows.map((row) => (
                  <li
                    key={row.key}
                    className="flex items-baseline justify-between gap-3 px-3 py-1.5"
                  >
                    <span>
                      {row.label}
                      {row.keywords.length > 0 && (
                        <span className="text-base-content/60"> · {row.keywords.join(", ")}</span>
                      )}
                    </span>
                    <strong className="tabular-nums">{formatBuff(row.power, row.toughness)}</strong>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  )
}
