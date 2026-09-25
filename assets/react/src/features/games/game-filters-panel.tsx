import { useQuery } from "@tanstack/react-query"
import { ChevronDown, X } from "lucide-react"
import { useEffect, useState, type ReactNode } from "react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import { COLOR_IDENTITIES, COLOR_NAMES } from "@/lib/color-identities"
import {
  WEEKDAY_NAMES,
  colorChoice,
  colorChoicePatch,
  gameFilterChips,
  hourLabel,
  moreFilterKeys,
  toGameFilters,
  type GameFilterKey,
  type GameFilters,
  type GamesSearch,
} from "./game-filters"
import { WIN_CONDITIONS, getPlayers, winConditionLabel, type WinCondition } from "./games"

const seatCounts = ["2", "3", "4", "5", "6"]
const seatNumbers = ["1", "2", "3", "4", "5", "6"]

/**
 * The games list's filter form. Every filter a stats link can set has a control here, and
 * every active filter is listed as a removable chip so the page states what it is showing.
 */
export function GameFiltersPanel({
  search,
  total,
  onChange,
  onClear,
}: {
  search: GamesSearch
  /** Matching games, once loaded. */
  total: number | undefined
  onChange: (patch: Partial<GameFilters>) => void
  onClear: () => void
}) {
  const filters = toGameFilters(search)
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const playerName = (id: number) => players.data?.find((player) => player.id === id)?.name
  const chips = gameFilterChips(search, {
    player: playerName,
    winCondition: (condition) => winConditionLabel(condition as WinCondition),
  })
  const color = colorChoice(search)

  const moreActive = moreFilterKeys.some((key) => search[key] !== undefined)
  const [showMore, setShowMore] = useState(false)
  const moreOpen = showMore || moreActive

  // A win/loss filter only means something for a chosen player.
  function remove(key: GameFilterKey) {
    onChange(key === "player_id" ? { player_id: "", player_result: "" } : { [key]: "" })
  }

  // Commander is free text; wait for a pause in typing before writing it to the URL.
  const [commander, setCommander] = useState(filters.commander)
  const urlCommander = filters.commander
  useEffect(() => {
    if (commander.trim() === urlCommander) return
    const timer = window.setTimeout(() => onChange({ commander: commander.trim() }), 250)
    return () => window.clearTimeout(timer)
  }, [commander, urlCommander])
  // The URL changed underneath the input (back button, external link, chip): follow it.
  const [seenUrlCommander, setSeenUrlCommander] = useState(urlCommander)
  if (urlCommander !== seenUrlCommander) {
    setSeenUrlCommander(urlCommander)
    if (commander.trim() !== urlCommander) setCommander(urlCommander)
  }

  const playerOptions = players.data?.map((player) => (
    <option key={player.id} value={player.id}>
      {player.name}
    </option>
  ))

  return (
    <section aria-label="Game filters" className="card border-base-300 bg-base-200 border">
      <div className="card-body gap-3 p-4">
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
          <Field label="Player">
            <select
              className="select select-bordered select-sm w-full"
              value={filters.player_id}
              onChange={(event) =>
                onChange({
                  player_id: event.target.value,
                  ...(event.target.value ? {} : { player_result: "" }),
                })
              }
            >
              <option value="">Everyone</option>
              {playerOptions}
            </select>
          </Field>
          <Field label="Winner">
            <select
              className="select select-bordered select-sm w-full"
              value={filters.winner_id}
              onChange={(event) => onChange({ winner_id: event.target.value })}
            >
              <option value="">Anyone</option>
              {playerOptions}
            </select>
          </Field>
          <Field label="Commander">
            <input
              type="search"
              className="input input-bordered input-sm w-full"
              placeholder="Any commander or partner"
              value={commander}
              onChange={(event) => setCommander(event.target.value)}
            />
          </Field>
          <div className="form-control min-w-0">
            <div className="mb-1 flex items-center justify-between gap-2">
              <label htmlFor="game-filter-colors" className="label-text text-xs font-semibold">
                Colors
              </label>
              <ToggleGroup
                type="single"
                value={color.won ? "won" : "played"}
                aria-label="Color filter applies to"
                className="join -my-1"
                disabled={!color.value}
                onValueChange={(value) => {
                  if (value) onChange(colorChoicePatch({ ...color, won: value === "won" }))
                }}
              >
                {(["played", "won"] as const).map((value) => (
                  <ToggleGroupItem
                    key={value}
                    value={value}
                    title={value === "played" ? "Any seat played it" : "The winner played it"}
                    className={cn(
                      "btn btn-xs join-item h-5 min-h-5 px-2",
                      (color.won ? "won" : "played") === value && color.value
                        ? "btn-primary"
                        : "btn-ghost",
                    )}
                  >
                    {value === "played" ? "In game" : "Won"}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </div>
            <select
              id="game-filter-colors"
              className="select select-bordered select-sm w-full"
              value={color.value}
              onChange={(event) =>
                onChange(colorChoicePatch({ value: event.target.value, won: color.won }))
              }
            >
              <option value="">Any colors</option>
              <optgroup label="Includes a color">
                {Object.entries(COLOR_NAMES).map(([letter, name]) => (
                  <option key={letter} value={`has:${letter}`}>
                    Includes {name}
                  </option>
                ))}
              </optgroup>
              <optgroup label="Exact identity">
                {COLOR_IDENTITIES.map(([identity, name]) => (
                  <option key={identity} value={identity}>
                    {name}
                  </option>
                ))}
              </optgroup>
            </select>
          </div>
          <Field label="Win condition">
            <select
              className="select select-bordered select-sm w-full"
              value={filters.win_condition}
              onChange={(event) => onChange({ win_condition: event.target.value })}
            >
              <option value="">Any ending</option>
              {WIN_CONDITIONS.map(([condition, label]) => (
                <option key={condition} value={condition}>
                  {label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Pod size">
            <select
              className="select select-bordered select-sm w-full"
              value={filters.player_count}
              onChange={(event) => onChange({ player_count: event.target.value })}
            >
              <option value="">Any size</option>
              {seatCounts.map((count) => (
                <option key={count} value={count}>
                  {count} players
                </option>
              ))}
            </select>
          </Field>
          <Field label="From">
            <input
              type="date"
              className="input input-bordered input-sm w-full"
              value={filters.date_from}
              onChange={(event) => onChange({ date_from: event.target.value })}
            />
          </Field>
          <Field label="Through">
            <input
              type="date"
              className="input input-bordered input-sm w-full"
              value={filters.date_to}
              onChange={(event) => onChange({ date_to: event.target.value })}
            />
          </Field>
          <RangeFilter
            label="Turns"
            min={filters.min_turns}
            max={filters.max_turns}
            onChange={(min, max) => onChange({ min_turns: min, max_turns: max })}
          />
          <RangeFilter
            label="Duration (minutes)"
            min={filters.min_duration}
            max={filters.max_duration}
            onChange={(min, max) => onChange({ min_duration: min, max_duration: max })}
          />
        </div>

        {moreActive ? (
          // An active filter below keeps the section open, so there is nothing to toggle.
          <p className="text-base-content/70 px-2 text-xs font-semibold">More filters</p>
        ) : (
          <button
            type="button"
            className="btn btn-ghost btn-xs self-start"
            aria-expanded={moreOpen}
            aria-controls="more-game-filters"
            onClick={() => setShowMore((open) => !open)}
          >
            <ChevronDown
              className={cn("size-3.5 transition-transform", moreOpen && "rotate-180")}
            />
            More filters
          </button>
        )}
        {moreOpen && (
          <div id="more-game-filters" className="grid grid-cols-2 gap-3 lg:grid-cols-5">
            <Field label="With player">
              <select
                className="select select-bordered select-sm w-full"
                value={filters.opponent_id}
                onChange={(event) => onChange({ opponent_id: event.target.value })}
              >
                <option value="">Anyone</option>
                {playerOptions}
              </select>
            </Field>
            <Field label="Player's result">
              <select
                className="select select-bordered select-sm w-full"
                value={filters.player_result}
                disabled={!filters.player_id}
                title={filters.player_id ? undefined : "Choose a player first"}
                onChange={(event) => onChange({ player_result: event.target.value })}
              >
                <option value="">Any result</option>
                <option value="win">Won</option>
                <option value="loss">Lost</option>
                <option value="draw">Drew</option>
              </select>
            </Field>
            <Field label="Winning seat">
              <select
                className="select select-bordered select-sm w-full"
                value={filters.winner_seat}
                onChange={(event) => onChange({ winner_seat: event.target.value })}
              >
                <option value="">Any seat</option>
                {seatNumbers.map((seat) => (
                  <option key={seat} value={seat}>
                    Seat {seat}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Weekday">
              <select
                className="select select-bordered select-sm w-full"
                value={filters.weekday}
                onChange={(event) => onChange({ weekday: event.target.value })}
              >
                <option value="">Any day</option>
                {WEEKDAY_NAMES.map((name, index) => (
                  <option key={name} value={index}>
                    {name}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Hour played">
              <select
                className="select select-bordered select-sm w-full"
                value={filters.hour}
                onChange={(event) => onChange({ hour: event.target.value })}
              >
                <option value="">Any time</option>
                {Array.from({ length: 24 }, (_, hour) => (
                  <option key={hour} value={hour}>
                    {hourLabel(hour)}–{hourLabel((hour + 1) % 24)}
                  </option>
                ))}
              </select>
            </Field>
          </div>
        )}

        {chips.length > 0 && (
          <div className="border-base-300 flex flex-wrap items-center gap-2 border-t pt-3 text-sm">
            <span className="text-base-content/70">
              {total === undefined ? "Showing" : `${total} ${total === 1 ? "game" : "games"}`}
            </span>
            <ul aria-label="Active filters" className="flex flex-1 flex-wrap gap-2">
              {chips.map((chip) => (
                <li key={chip.key}>
                  <button
                    type="button"
                    className="badge badge-primary badge-outline hover:bg-primary/10 gap-1 py-3"
                    aria-label={`Remove filter: ${chip.label}`}
                    onClick={() => remove(chip.key)}
                  >
                    {chip.label}
                    <X aria-hidden="true" className="size-3" />
                  </button>
                </li>
              ))}
            </ul>
            <button type="button" className="btn btn-ghost btn-sm" onClick={onClear}>
              <X className="size-4" /> Clear all
            </button>
          </div>
        )}
      </div>
    </section>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="form-control min-w-0">
      <span className="label-text mb-1 text-xs font-semibold">{label}</span>
      {children}
    </label>
  )
}

/** Paired min/max numeric inputs that share one label. */
function RangeFilter({
  label,
  min,
  max,
  onChange,
}: {
  label: string
  min: string
  max: string
  onChange: (min: string, max: string) => void
}) {
  return (
    <fieldset className="form-control min-w-0">
      <legend className="label-text mb-1 text-xs font-semibold">{label}</legend>
      <div className="join w-full">
        <input
          type="number"
          inputMode="numeric"
          min={1}
          className="input input-bordered input-sm join-item w-full min-w-0"
          placeholder="Min"
          aria-label={`Minimum ${label.toLowerCase()}`}
          value={min}
          onChange={(event) => onChange(event.target.value, max)}
        />
        <input
          type="number"
          inputMode="numeric"
          min={1}
          className="input input-bordered input-sm join-item w-full min-w-0"
          placeholder="Max"
          aria-label={`Maximum ${label.toLowerCase()}`}
          value={max}
          onChange={(event) => onChange(min, event.target.value)}
        />
      </div>
    </fieldset>
  )
}
