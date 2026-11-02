import { act, renderHook } from "@testing-library/react"
import { expect, it } from "vite-plus/test"
import type { RoomLink } from "./room-link"
import type { TableChannel } from "./table-channel"
import { leadingChoices, voteRequest } from "./table-votes"
import { useTableVote, type TableVote } from "./use-table-vote"

function vote(id: string, closed: boolean): TableVote {
  return {
    id,
    question: "Who is the threat?",
    choices: ["Alice", "Bob"],
    secret: false,
    audience: "everyone",
    creator: { player_id: 1, player_name: "Alice" },
    voters: [{ player_id: 2, player_name: "Bob", voted: closed, choice: closed ? 1 : null }],
    closed,
    tally: closed ? [0, 1] : null,
  }
}

it("pops each vote's ballot and results once, but not results that closed before a rejoin", () => {
  const listeners = new Map<string, (payload: unknown) => void>()
  const channel = { on: (event: string, callback: never) => listeners.set(event, callback) }
  const link: RoomLink = { peerId: "peer", channel: null, spectator: false, participants: [] }
  const { result } = renderHook(() => useTableVote(link))
  act(() => result.current.bindChannel(channel as unknown as TableChannel))
  const fire = (event: string, payload: TableVote | null) =>
    act(() => listeners.get(event)!({ vote: payload }))

  fire("vote_state", vote("old", true))
  expect(result.current.dialogDismissed).toBe(true)

  fire("vote", vote("next", false))
  expect(result.current.dialogDismissed).toBe(false)
  act(() => result.current.dismissDialog())
  expect(result.current.dialogDismissed).toBe(true)
  // Closing the ballot does not hide the results.
  fire("vote", vote("next", true))
  expect(result.current.dialogDismissed).toBe(false)
  act(() => result.current.dismissDialog())
  expect(result.current.dialogDismissed).toBe(true)

  // A rejoin while a vote is open brings the ballot back.
  fire("vote_state", vote("last", false))
  expect(result.current.dialogDismissed).toBe(false)
})

it("builds a vote request from the form, dropping blank choices", () => {
  expect(voteRequest(" Threat? ", ["Alice ", "", " Bob", "  "], true, "opponents")).toEqual({
    question: "Threat?",
    choices: ["Alice", "Bob"],
    secret: true,
    audience: "opponents",
  })
  expect(voteRequest("  ", ["A", "B"], false, "everyone")).toBe("Ask a question.")
  expect(voteRequest("Q", ["A", " "], false, "everyone")).toBe("Add at least two choices.")
  expect(voteRequest("Q", ["A", "B", " A"], false, "everyone")).toBe(
    "Each choice must be different.",
  )
})

it("leads with every tied top choice, and none before anyone voted", () => {
  expect(leadingChoices([1, 3, 0, 3])).toEqual([1, 3])
  expect(leadingChoices([0, 0])).toEqual([])
})
