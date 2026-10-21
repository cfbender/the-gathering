import { Link } from "@tanstack/react-router"
import { useMutation, useMutationState, useQuery, useQueryClient } from "@tanstack/react-query"
import { Eraser, Scale } from "lucide-react"
import { useEffect, useState, type FormEvent, type KeyboardEvent } from "react"
import { Textarea } from "@/components/ui/textarea"
import type { DeckSummary } from "@/features/decks/decks"
import { RulingAnswer } from "@/features/rulings/ruling-answer"
import {
  askRuling,
  formatDate,
  formatUsd,
  getRulingStatus,
  rulingErrorMessage,
  rulingStatusKey,
  type RulingStatus,
  type RulingTurn,
} from "@/features/rulings/rulings"
import { errorMessage, useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"
import { PanelSection } from "./panel-section"
import type { BoardCard, TableParticipant } from "./use-webcam-room"

export interface RulingsTabProps {
  roomId: string
  /** Commanders and identified cards at the table, sent as context. */
  tableCards: string[]
}

/** The commanders of the seated decks and every identified card, once each. */
export function tableCardNames(
  seated: TableParticipant[],
  decks: DeckSummary[],
  identifiedCards: BoardCard[],
) {
  const names = seated.flatMap((participant) => {
    const deck = decks.find((deck) => deck.id === participant.deck_id)
    return deck ? [deck.commander_name, deck.partner_name] : []
  })
  names.push(...identifiedCards.map((entry) => entry.card.name))
  return [...new Set(names.filter((name): name is string => !!name))]
}

const storageKey = (roomId: string) => `the-gathering:rulings:${roomId}`

/** The conversation lives in session storage, so it survives tab switches and reloads. */
function readConversation(roomId: string): RulingTurn[] {
  try {
    const stored = JSON.parse(sessionStorage.getItem(storageKey(roomId)) ?? "[]") as unknown
    return Array.isArray(stored) ? (stored as RulingTurn[]) : []
  } catch {
    return []
  }
}

function writeConversation(roomId: string, turns: RulingTurn[]) {
  try {
    if (turns.length === 0) sessionStorage.removeItem(storageKey(roomId))
    else sessionStorage.setItem(storageKey(roomId), JSON.stringify(turns))
  } catch {
    // Storage may be unavailable; the conversation then lasts until the tab closes.
  }
}

function UsageLine({ status }: { status: RulingStatus }) {
  const { spent_usd, limit_usd, resets_on } = status.usage
  const rules = status.rules_effective_on
    ? `Comprehensive Rules of ${formatDate(status.rules_effective_on)}`
    : "Comprehensive Rules not downloaded yet"
  const spent =
    limit_usd === null
      ? `${formatUsd(spent_usd)} used this month`
      : `${formatUsd(spent_usd)} of ${formatUsd(limit_usd)} used this month, resets ${formatDate(resets_on)}`
  return (
    <p className="text-base-content/50 text-[0.65rem]">
      {rules} · {spent}
    </p>
  )
}

/** Side-panel tab for asking an AI judge, which looks up the Comprehensive Rules and the
 * catalog's Oracle text and rulings before answering. */
export function RulingsTab({ roomId, tableCards }: RulingsTabProps) {
  const queryClient = useQueryClient()
  const user = useCurrentUser()
  const status = useQuery({ queryKey: rulingStatusKey, queryFn: getRulingStatus })
  const [turns, setTurns] = useState(() => readConversation(roomId))
  const [question, setQuestion] = useState("")
  // Sending the table's cards to the model is opt-in.
  const [includeCards, setIncludeCards] = useState(false)
  const mutationKey = ["ai-rulings", roomId]
  const ask = useMutation({
    mutationKey,
    mutationFn: async (text: string) => {
      const turn = await askRuling(text, readConversation(roomId), includeCards ? tableCards : [])
      writeConversation(roomId, [...readConversation(roomId), turn])
      return turn
    },
    onSuccess: () => setQuestion(""),
    onSettled: () => {
      setTurns(readConversation(roomId))
      void queryClient.invalidateQueries({ queryKey: rulingStatusKey })
    },
  })
  // A question asked before switching tabs keeps going; show it and pick up its answer.
  const pending = useMutationState({
    filters: { mutationKey, status: "pending" },
    select: (mutation) => mutation.state.variables as string,
  })
  useEffect(() => {
    if (pending.length === 0) setTurns(readConversation(roomId))
  }, [pending.length, roomId])

  function submit(event?: FormEvent) {
    event?.preventDefault()
    const text = question.trim()
    if (text && pending.length === 0) ask.mutate(text)
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      submit()
    }
  }

  const data = status.data
  return (
    <PanelSection title="Ask for a ruling" icon={Scale}>
      {status.isPending && <div className="skeleton h-16 w-full" />}
      {status.error && <p className="text-error text-xs">{errorMessage(status.error)}</p>}
      {data && !data.enabled && (
        <p className="text-base-content/60 text-xs">
          AI rulings are not set up on this server.{" "}
          {user.data?.role === "admin" ? (
            <>
              Add an OpenRouter API key and model in{" "}
              <Link to="/admin/settings" className="link">
                Admin › Settings
              </Link>
              .
            </>
          ) : (
            "An administrator can turn them on."
          )}
        </p>
      )}
      {data?.enabled && (
        <div className="grid gap-3">
          {turns.length === 0 && pending.length === 0 && (
            <p className="text-base-content/60 text-xs">
              Ask any rules question. The AI judge reads the Comprehensive Rules and each
              card&apos;s Oracle text and rulings before answering, and cites the rules it used.
            </p>
          )}
          <ol className="grid gap-3" aria-label="Rulings conversation">
            {turns.map((turn, index) => (
              <li key={index} className="grid gap-1.5">
                <p className="bg-primary/15 ml-6 justify-self-end rounded-box px-2.5 py-1.5 text-xs whitespace-pre-wrap">
                  {turn.question}
                </p>
                <RulingAnswer turn={turn} />
              </li>
            ))}
            {pending.map((text, index) => (
              <li key={`pending-${index}`} className="grid gap-1.5">
                <p className="bg-primary/15 ml-6 justify-self-end rounded-box px-2.5 py-1.5 text-xs whitespace-pre-wrap">
                  {text}
                </p>
                <p role="status" className="text-base-content/60 flex items-center gap-2 text-xs">
                  <span className="loading loading-dots loading-xs" />
                  Looking up rules and cards…
                </p>
              </li>
            ))}
          </ol>
          {ask.error && (
            <p role="alert" className="text-error text-xs">
              {rulingErrorMessage(ask.error)}
            </p>
          )}
          <form className="grid gap-2" onSubmit={submit}>
            <Textarea
              className="min-h-16 text-xs"
              rows={3}
              maxLength={2000}
              value={question}
              onChange={(event) => setQuestion(event.target.value)}
              onKeyDown={onKeyDown}
              placeholder={
                turns.length > 0
                  ? "Ask a follow-up…"
                  : "If my deathtouch creature with trample is blocked by a 5/5, how much damage tramples over?"
              }
              aria-label="Rules question"
            />
            <div className="flex flex-wrap items-center gap-2">
              {tableCards.length > 0 && (
                <label className="text-base-content/70 flex cursor-pointer items-center gap-1.5 text-[0.65rem]">
                  <input
                    type="checkbox"
                    className="checkbox checkbox-xs"
                    checked={includeCards}
                    onChange={(event) => setIncludeCards(event.target.checked)}
                  />
                  Include the {tableCards.length} card{tableCards.length === 1 ? "" : "s"} in play
                </label>
              )}
              {turns.length > 0 && (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm ml-auto gap-1"
                  onClick={() => {
                    writeConversation(roomId, [])
                    setTurns([])
                    ask.reset()
                  }}
                >
                  <Eraser className="size-3.5" /> New conversation
                </button>
              )}
              <button
                type="submit"
                className={cn("btn btn-primary btn-sm", turns.length === 0 && "ml-auto")}
                disabled={!question.trim() || pending.length > 0}
              >
                Ask
              </button>
            </div>
          </form>
          <UsageLine status={data} />
          <p className="text-base-content/45 text-[0.65rem]">
            AI can be wrong. Check the cited rules when it matters.
          </p>
        </div>
      )}
    </PanelSection>
  )
}
