import { Plus, Vote as VoteIcon, X } from "lucide-react"
import { useState, type ReactNode } from "react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Switch } from "@/components/ui/switch"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/cn"
import { PanelSection } from "./panel-section"
import type { TableView } from "./table-view"
import type { TableVote, VoteAudience, VoteRequest } from "./use-table-vote"

const MAX_CHOICES = 20

/** The request for a filled-in form, or why it cannot be sent. Blank extra choices are dropped. */
export function voteRequest(
  question: string,
  choices: string[],
  secret: boolean,
  audience: VoteAudience,
): VoteRequest | string {
  const asked = question.trim()
  const answers = choices.map((choice) => choice.trim()).filter(Boolean)
  if (!asked) return "Ask a question."
  if (answers.length < 2) return "Add at least two choices."
  if (new Set(answers).size < answers.length) return "Each choice must be different."
  return { question: asked, choices: answers, secret, audience }
}

/** The choices with the most answers; none before anyone answered. */
export function leadingChoices(tally: number[]): number[] {
  const top = Math.max(0, ...tally)
  return top === 0 ? [] : tally.flatMap((count, index) => (count === top ? [index] : []))
}

function CardChoice<T extends string>({
  label,
  value,
  options,
  disabled,
  onChange,
  className,
}: {
  label: string
  value: T | null
  options: readonly (readonly [T, string, ReactNode?])[]
  disabled?: boolean
  onChange: (value: T) => void
  className?: string
}) {
  return (
    <ToggleGroup
      type="single"
      value={value ?? ""}
      onValueChange={(next) => {
        if (next) onChange(next as T)
      }}
      aria-label={label}
      className={cn("grid gap-2", className)}
    >
      {options.map(([option, title, hint]) => (
        <ToggleGroupItem
          key={option}
          value={option}
          disabled={disabled}
          className={cn(
            "btn h-auto min-h-11 flex-col items-start gap-0.5 px-4 py-2 text-left font-semibold",
            value === option ? "btn-primary" : "btn-outline",
          )}
        >
          <span className="break-words whitespace-normal">{title}</span>
          {hint && <span className="text-xs font-normal opacity-65">{hint}</span>}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}

/** The Table tab's entry point. */
export function TableVoteSection({
  voteOpen,
  onStart,
}: {
  voteOpen: boolean
  onStart: () => void
}) {
  return (
    <PanelSection title="Vote" icon={VoteIcon}>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="w-full"
        disabled={voteOpen}
        onClick={onStart}
      >
        <VoteIcon className="size-3.5" />
        {voteOpen ? "A vote is in progress" : "Start a vote…"}
      </Button>
    </PanelSection>
  )
}

/** Asks the table a question: the asker picks who answers and whether answers stay secret. */
export function StartVoteDialog({
  onStart,
  onClose,
}: {
  onStart: (request: VoteRequest) => Promise<string | null>
  onClose: () => void
}) {
  const [question, setQuestion] = useState("")
  const [choices, setChoices] = useState(["", ""])
  const [secret, setSecret] = useState(false)
  const [audience, setAudience] = useState<VoteAudience>("everyone")
  const [error, setError] = useState<string | null>(null)
  const [sending, setSending] = useState(false)

  async function submit() {
    const request = voteRequest(question, choices, secret, audience)
    if (typeof request === "string") {
      setError(request)
      return
    }
    setSending(true)
    const refused = await onStart(request)
    setSending(false)
    if (refused) setError(refused)
    else onClose()
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Start a vote</DialogTitle>
          <DialogClose onClose={onClose} />
        </DialogHeader>
        <form
          className="grid gap-5 p-5 sm:p-6"
          onSubmit={(event) => {
            event.preventDefault()
            void submit()
          }}
        >
          <fieldset disabled={sending} className="grid min-w-0 gap-5">
            <label className="form-control">
              <span className="mb-1 text-sm font-bold">Question</span>
              <input
                className="input input-bordered w-full"
                value={question}
                maxLength={200}
                placeholder="Who is the biggest threat?"
                autoFocus
                onChange={(event) => setQuestion(event.target.value)}
              />
            </label>
            <fieldset className="grid gap-2">
              <legend className="mb-2 text-sm font-bold">Choices</legend>
              {choices.map((choice, index) => (
                <div key={index} className="flex items-center gap-2">
                  <input
                    className="input input-bordered input-sm min-w-0 flex-1"
                    value={choice}
                    maxLength={100}
                    aria-label={`Choice ${index + 1}`}
                    placeholder={`Choice ${index + 1}`}
                    onChange={(event) =>
                      setChoices(
                        choices.map((current, at) => (at === index ? event.target.value : current)),
                      )
                    }
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="btn-sm"
                    aria-label={`Remove choice ${index + 1}`}
                    disabled={choices.length <= 2}
                    onClick={() => setChoices(choices.filter((_, at) => at !== index))}
                  >
                    <X className="size-4" />
                  </Button>
                </div>
              ))}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="justify-self-start"
                disabled={choices.length >= MAX_CHOICES}
                onClick={() => setChoices([...choices, ""])}
              >
                <Plus className="size-4" /> Add choice
              </Button>
            </fieldset>
            <fieldset>
              <legend className="mb-2 text-sm font-bold">Who votes</legend>
              <CardChoice
                label="Who votes"
                value={audience}
                className="sm:grid-cols-2"
                options={[
                  ["everyone", "Everyone", "Every player still in the game, you included"],
                  ["opponents", "Opponents only", "Everyone but you"],
                ]}
                onChange={setAudience}
              />
            </fieldset>
            <label className="flex items-center justify-between gap-4">
              <span>
                <span className="block text-sm font-bold">Secret vote</span>
                <span className="text-base-content/65 block text-xs">
                  {secret
                    ? "Results show only the totals, never who chose what."
                    : "Results show who chose what."}
                </span>
              </span>
              <Switch checked={secret} onCheckedChange={setSecret} aria-label="Secret vote" />
            </label>
          </fieldset>
          {error && (
            <p role="alert" className="text-error text-sm">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <button type="button" className="btn btn-ghost" onClick={onClose} disabled={sending}>
              Cancel
            </button>
            <button type="submit" className="btn btn-primary" disabled={sending}>
              {sending ? "Starting…" : "Start vote"}
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function voteKind(vote: TableVote) {
  return vote.secret ? "Secret vote" : "Open vote"
}

function progress(vote: TableVote) {
  const answered = vote.voters.filter((voter) => voter.voted).length
  return `${answered} of ${vote.voters.length} voted`
}

function BallotDialog({
  vote,
  onCast,
  onClose,
}: {
  vote: TableVote
  onCast: (choice: number) => Promise<string | null>
  onClose: () => void
}) {
  const [choice, setChoice] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [sending, setSending] = useState(false)

  async function submit() {
    if (choice === null) return
    setSending(true)
    const refused = await onCast(choice)
    setSending(false)
    if (refused) setError(refused)
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="min-w-0">
            <p className="text-base-content/60 text-xs font-bold tracking-wide uppercase">
              {voteKind(vote)} from {vote.creator.player_name}
            </p>
            <DialogTitle className="mt-1 break-words">{vote.question}</DialogTitle>
          </div>
          <DialogClose onClose={onClose} />
        </DialogHeader>
        <form
          className="grid gap-4 p-5 sm:p-6"
          onSubmit={(event) => {
            event.preventDefault()
            void submit()
          }}
        >
          <CardChoice
            label={vote.question}
            value={choice === null ? null : String(choice)}
            disabled={sending}
            options={vote.choices.map((text, index) => [String(index), text] as const)}
            onChange={(next) => setChoice(Number(next))}
          />
          <p className="text-base-content/65 text-xs">
            {vote.secret
              ? "Nobody will see your choice. Results show only the totals."
              : "Choices stay hidden until everyone has voted, then the results show who chose what."}{" "}
            {progress(vote)}.
          </p>
          {error && (
            <p role="alert" className="text-error text-sm">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <button type="button" className="btn btn-ghost" onClick={onClose}>
              Later
            </button>
            <button type="submit" className="btn btn-primary" disabled={choice === null || sending}>
              {sending ? "Voting…" : "Submit vote"}
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ResultsDialog({ vote, onClose }: { vote: TableVote; onClose: () => void }) {
  const tally = vote.tally ?? vote.choices.map(() => 0)
  const leading = leadingChoices(tally)
  const total = tally.reduce((sum, count) => sum + count, 0)
  const missing = vote.voters.filter((voter) => !voter.voted)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="min-w-0">
            <p className="text-base-content/60 text-xs font-bold tracking-wide uppercase">
              {voteKind(vote)} results
            </p>
            <DialogTitle className="mt-1 break-words">{vote.question}</DialogTitle>
          </div>
          <DialogClose onClose={onClose} />
        </DialogHeader>
        <div className="grid gap-4 p-5 sm:p-6">
          <ol className="grid gap-3" aria-label="Results">
            {vote.choices.map((choice, index) => {
              const count = tally[index] ?? 0
              const lead = leading.includes(index)
              const names = vote.voters
                .filter((voter) => voter.choice === index)
                .map((voter) => voter.player_name)
              return (
                <li key={choice} className="grid gap-1">
                  <div className="flex items-baseline justify-between gap-3">
                    <span className={cn("min-w-0 break-words", lead && "text-accent font-bold")}>
                      {choice}
                    </span>
                    <span className={cn("shrink-0 text-sm tabular-nums", lead && "font-bold")}>
                      {count} {count === 1 ? "vote" : "votes"}
                    </span>
                  </div>
                  <div className="bg-base-300 h-2 overflow-hidden rounded-full">
                    <div
                      className={cn(
                        "h-full rounded-full",
                        lead ? "bg-accent" : "bg-base-content/30",
                      )}
                      style={{ width: total ? `${(count / total) * 100}%` : 0 }}
                    />
                  </div>
                  {names.length > 0 && (
                    <span className="text-base-content/65 text-xs">{names.join(", ")}</span>
                  )}
                </li>
              )
            })}
          </ol>
          {missing.length > 0 && (
            <p className="text-base-content/65 text-xs">
              Ended early. Did not vote: {missing.map((voter) => voter.player_name).join(", ")}
            </p>
          )}
          <div className="flex justify-end">
            <button type="button" className="btn btn-primary" onClick={onClose}>
              Close
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** The table's vote on the stage: a progress banner while it is open, the ballot for players
 * who still have to answer, and the results for everyone once it closes. */
export function TableVoteOverlay({ view }: { view: TableView }) {
  const { room, playerId } = view
  const { votes } = room
  const [endError, setEndError] = useState<{ voteId: string; reason: string } | null>(null)
  const vote = votes.vote
  if (!vote) return null
  const error = endError?.voteId === vote.id ? endError.reason : null
  const voter = vote.voters.find((candidate) => candidate.player_id === playerId)
  const mustVote = !vote.closed && !room.spectating && voter !== undefined && !voter.voted
  const canEnd = !vote.closed && (room.isOwner || vote.creator.player_id === playerId)

  return (
    <>
      {!vote.closed && (
        <div
          role="status"
          className="absolute top-3 left-1/2 z-30 flex w-max max-w-[92%] -translate-x-1/2 flex-wrap items-center justify-center gap-x-3 gap-y-1 rounded-xl border border-accent/40 bg-base-100/95 px-4 py-2 text-sm text-base-content shadow-xl"
        >
          <VoteIcon className="text-accent size-4 shrink-0" aria-hidden="true" />
          <span className="min-w-0 truncate font-semibold">
            {vote.creator.player_name}: {vote.question}
          </span>
          <span className="text-base-content/65 shrink-0 text-xs">{progress(vote)}</span>
          {mustVote && votes.dialogDismissed && (
            <Button type="button" size="sm" onClick={votes.reopenDialog}>
              Vote now
            </Button>
          )}
          {canEnd && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                void votes
                  .end(vote.id)
                  .then((reason) => setEndError(reason ? { voteId: vote.id, reason } : null))
              }}
            >
              End vote
            </Button>
          )}
          {error && (
            <span role="alert" className="text-error w-full text-center text-xs">
              {error}
            </span>
          )}
        </div>
      )}
      {mustVote && !votes.dialogDismissed && (
        <BallotDialog
          key={vote.id}
          vote={vote}
          onCast={(choice) => votes.cast(vote.id, choice)}
          onClose={votes.dismissDialog}
        />
      )}
      {vote.closed && !votes.dialogDismissed && (
        <ResultsDialog vote={vote} onClose={votes.dismissDialog} />
      )}
    </>
  )
}
