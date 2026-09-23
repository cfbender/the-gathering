import type { DeckSummary } from "@/features/decks/decks"

export interface SeatCounters {
  poison: number
  rad: number
  commander_casts: Record<string, number>
  /** Player id, then commander name: identical commanders at two seats stay separate. */
  commander_damage: Record<string, Record<string, number>>
}

export const EMPTY_COUNTERS: SeatCounters = {
  poison: 0,
  rad: 0,
  commander_casts: {},
  commander_damage: {},
}

export type Counter =
  | { kind: "poison" | "rad" }
  | { kind: "casts"; commander: string }
  | { kind: "damage"; playerId: number; commander: string }

export function commanderNames(
  deck: Pick<DeckSummary, "commander_name" | "partner_name"> | undefined,
) {
  return deck
    ? [deck.commander_name, deck.partner_name].filter((name): name is string => !!name)
    : []
}

export function counterValue(state: SeatCounters, counter: Counter): number {
  switch (counter.kind) {
    case "poison":
    case "rad":
      return state[counter.kind]
    case "casts":
      return state.commander_casts[counter.commander] ?? 0
    case "damage":
      return state.commander_damage[counter.playerId]?.[counter.commander] ?? 0
  }
}

export function changeCounter(state: SeatCounters, counter: Counter, delta: number): SeatCounters {
  const value = Math.max(0, Math.min(999, counterValue(state, counter) + delta))
  switch (counter.kind) {
    case "poison":
    case "rad":
      return { ...state, [counter.kind]: value }
    case "casts":
      return { ...state, commander_casts: { ...state.commander_casts, [counter.commander]: value } }
    case "damage":
      return {
        ...state,
        commander_damage: {
          ...state.commander_damage,
          [counter.playerId]: {
            ...state.commander_damage[counter.playerId],
            [counter.commander]: value,
          },
        },
      }
  }
}

export function counterWarning(state: SeatCounters) {
  return (
    state.poison >= 10 ||
    Object.values(state.commander_damage).some((counts) =>
      Object.values(counts).some((damage) => damage >= 21),
    )
  )
}

export function describeCounterChanges(
  previous: SeatCounters,
  next: SeatCounters,
  playerName: string,
  players: { player_id: number; player_name: string }[],
) {
  const lines: string[] = []
  const add = (label: string, before: number, after: number) => {
    if (before !== after) lines.push(`${playerName} ${label}: ${before} → ${after}`)
  }
  add("poison", previous.poison, next.poison)
  add("rad", previous.rad, next.rad)
  for (const name of new Set([
    ...Object.keys(previous.commander_casts),
    ...Object.keys(next.commander_casts),
  ])) {
    add(
      `${name} commander tax`,
      (previous.commander_casts[name] ?? 0) * 2,
      (next.commander_casts[name] ?? 0) * 2,
    )
  }
  for (const id of new Set([
    ...Object.keys(previous.commander_damage),
    ...Object.keys(next.commander_damage),
  ])) {
    const before = previous.commander_damage[id] ?? {}
    const after = next.commander_damage[id] ?? {}
    const source =
      players.find((player) => String(player.player_id) === id)?.player_name ?? `player ${id}`
    for (const name of new Set([...Object.keys(before), ...Object.keys(after)])) {
      add(`damage from ${source}'s ${name}`, before[name] ?? 0, after[name] ?? 0)
    }
  }
  return lines
}
