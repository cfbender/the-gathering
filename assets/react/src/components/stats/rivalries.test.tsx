import { cleanup, render, screen, within } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import type { ComponentProps, ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import type { HeadToHead } from "@/lib/stats"
import type { GamesSearch } from "@/features/games/game-filters"
import { Rivalries } from "./rivalries"

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    search,
    to,
    ...props
  }: ComponentProps<"a"> & {
    search: GamesSearch
    to: string
  }) => (
    <a
      {...props}
      href={`${to}?${new URLSearchParams(Object.entries(search).map(([key, value]) => [key, String(value)]))}`}
    />
  ),
}))

afterEach(cleanup)

const games = { player_id: 9, date_from: "2026-02-01" }
const player = { name: "Drew", user_id: 42 }

function session(viewerId?: number) {
  const client = new QueryClient({ defaultOptions: { queries: { enabled: false } } })
  if (viewerId !== undefined) client.setQueryData(["session"], { id: viewerId })
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>
  }
}

function filters(name: string) {
  const url = new URL(
    screen.getByRole("link", { name }).getAttribute("href")!,
    "https://example.test",
  )
  expect(url.pathname).toBe("/games")
  return Object.fromEntries(url.searchParams)
}

const opponents: HeadToHead[] = [
  {
    id: 1,
    name: "Ari",
    avatar_url: null,
    games: 15,
    wins: 8,
    losses: 7,
    draws: 0,
    kills: 1,
    killed_by: 0,
  },
  {
    id: 2,
    name: "Bea",
    avatar_url: null,
    games: 10,
    wins: 2,
    losses: 1,
    draws: 0,
    kills: 4,
    killed_by: 1,
  },
  {
    id: 3,
    name: "Cal",
    avatar_url: null,
    games: 12,
    wins: 1,
    losses: 2,
    draws: 0,
    kills: 2,
    killed_by: 3,
  },
]

it.each([
  { viewerId: 42, userId: 42, label: "you", scenario: "own profile" },
  { viewerId: 9, userId: 42, label: "Drew", scenario: "another player's profile" },
  { viewerId: undefined, userId: 42, label: "Drew", scenario: "no signed-in user" },
  { viewerId: 9, userId: null, label: "Drew", scenario: "unlinked profile" },
  {
    viewerId: undefined,
    userId: null,
    label: "Drew",
    scenario: "unlinked profile without a viewer",
  },
])("labels and links rivalries for $scenario", ({ viewerId, userId, label }) => {
  render(
    <Rivalries
      player={{ ...player, user_id: userId }}
      games={games}
      headToHead={opponents}
      commanders={[
        {
          id: "kangee",
          name: "Kangee",
          art_crop_url: null,
          color_identity: "WU",
          faced: 12,
          beat_me: 3,
          beaten: 5,
        },
      ]}
    />,
    { wrapper: session(viewerId) },
  )
  expect(filters("Ari: 8 wins")).toEqual({
    date_from: "2026-02-01",
    player_id: "9",
    opponent_id: "1",
    player_result: "win",
  })
  expect(filters("Ari: 7 losses")).toEqual({
    date_from: "2026-02-01",
    player_id: "9",
    winner_id: "1",
  })
  expect(filters("Kangee: 3 losses")).toEqual({
    date_from: "2026-02-01",
    player_id: "9",
    opponent_commander: "Kangee",
    opponent_result: "win",
  })
  expect(filters("Kangee: 5 wins")).toEqual({
    date_from: "2026-02-01",
    player_id: "9",
    opponent_commander: "Kangee",
    player_result: "win",
  })
  expect(filters("Kangee: 12 faced. Show games")).toEqual({
    date_from: "2026-02-01",
    player_id: "9",
    opponent_commander: "Kangee",
  })
  expect(screen.getByText(label === "you" ? "Beat me" : "Wins")).toBeTruthy()
  expect(filters(`Kangee: 3 beat ${label}. Show games`)).toEqual(filters("Kangee: 3 losses"))
  expect(filters(`Kangee: 5 beaten by ${label}. Show games`)).toEqual(filters("Kangee: 5 wins"))
  const kills = within(screen.getByRole("region", { name: "Kill rivalries" }))
  expect(kills.getByText(`4 times killed by ${label}`)).toBeTruthy()
  expect(kills.getByText(`3 times killed ${label}`)).toBeTruthy()
  expect(filters(`Bea: 4 times killed by ${label}`)).toEqual({
    date_from: "2026-02-01",
    killer_id: "9",
    victim_id: "2",
  })
  expect(filters(`Cal: 3 times killed ${label}`)).toEqual({
    date_from: "2026-02-01",
    killer_id: "3",
    victim_id: "9",
  })
  expect(kills.getByText(/Number-only kills are not attributed/)).toBeTruthy()
})

it("does not name a zero-count leader, including when only one direction has attribution", () => {
  const { rerender } = render(
    <Rivalries
      player={player}
      games={games}
      headToHead={opponents.map((row) => ({ ...row, kills: 0, killed_by: 0 }))}
      commanders={[]}
    />,
    { wrapper: session(42) },
  )
  const kills = within(screen.getByRole("region", { name: "Kill rivalries" }))
  expect(kills.queryAllByRole("link")).toHaveLength(0)
  expect(kills.getByText("No named victims in this date range.")).toBeTruthy()
  expect(kills.getByText("No named killers in this date range.")).toBeTruthy()
  rerender(<Rivalries player={player} games={games} headToHead={[opponents[0]!]} commanders={[]} />)
  expect(kills.getByRole("link", { name: "Ari: 1 time killed by you" })).toBeTruthy()
  expect(kills.getByText("No named killers in this date range.")).toBeTruthy()
})
