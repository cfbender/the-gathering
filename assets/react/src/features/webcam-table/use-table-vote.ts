import { useCallback, useState } from "react"
import type { RoomLink } from "./room-link"
import type { TableChannel } from "./table-channel"

export type VoteAudience = "everyone" | "opponents"

export interface VoteRequest {
  question: string
  choices: string[]
  secret: boolean
  audience: VoteAudience
}

export interface VoteVoter {
  player_id: number
  player_name: string
  voted: boolean
  /** Only once an open (not secret) vote has closed. */
  choice: number | null
}

/** The table's open or latest vote, as the server sends it. */
export interface TableVote {
  id: string
  question: string
  choices: string[]
  secret: boolean
  audience: VoteAudience
  creator: { player_id: number; player_name: string }
  voters: VoteVoter[]
  closed: boolean
  /** Answers per choice, once closed. */
  tally: number[] | null
}

/** The dialog a vote shows: its ballot while open, its results once closed. */
export function voteDialogKey(vote: TableVote) {
  return `${vote.id}:${vote.closed ? "results" : "ballot"}`
}

/** Resolves with the server's refusal, or null once it accepted. */
function send(link: RoomLink, event: string, payload: unknown): Promise<string | null> {
  return new Promise((resolve) => {
    const channel = link.channel
    if (link.spectator || channel?.state !== "joined") {
      resolve("Reconnect to the table and try again")
      return
    }
    channel
      .push(event, payload)
      .receive("ok", () => resolve(null))
      .receive("error", ({ reason }: { reason: string }) => resolve(reason))
      .receive("timeout", () => resolve("The table did not answer; try again"))
  })
}

/** The table's vote and this seat's commands for it. The ballot pops up for voters while the
 * vote is open and the results for everyone when it closes, each until this player closes it. */
export function useTableVote(link: RoomLink) {
  const [vote, setVote] = useState<TableVote | null>(null)
  const [dismissed, setDismissed] = useState<string | null>(null)

  const bindChannel = useCallback((room: TableChannel) => {
    // Sent on every (re)join; results that closed before it are not news.
    room.on("vote_state", ({ vote: current }: { vote: TableVote | null }) => {
      setVote(current)
      if (current?.closed) setDismissed(voteDialogKey(current))
    })
    room.on("vote", ({ vote: current }: { vote: TableVote }) => setVote(current))
  }, [])

  return {
    vote,
    /** Whether this player closed the vote's current dialog. */
    dialogDismissed: vote !== null && dismissed === voteDialogKey(vote),
    dismissDialog: () => setDismissed(vote && voteDialogKey(vote)),
    reopenDialog: () => setDismissed(null),
    bindChannel,
    start: (request: VoteRequest) => send(link, "start_vote", request),
    cast: (voteId: string, choice: number) => send(link, "cast_vote", { vote_id: voteId, choice }),
    end: (voteId: string) => send(link, "end_vote", { vote_id: voteId }),
  }
}
