import { useMutation, useQueryClient } from "@tanstack/react-query"
import { X } from "lucide-react"
import { useState } from "react"
import { useToast } from "@/components/ui/toast"
import { invalidateGameRelated, type Game } from "@/features/games/games"
import { api } from "@/lib/api"
import { cn } from "@/lib/cn"

/** How much a player enjoyed a game: 1 (awful) to 5 (rad). Salt is the low end. */
export type Satisfaction = 1 | 2 | 3 | 4 | 5

export const SATISFACTION_LEVELS: { value: Satisfaction; label: string; color: string }[] = [
  { value: 1, label: "awful", color: "text-slate-500" },
  { value: 2, label: "bad", color: "text-blue-500" },
  { value: 3, label: "meh", color: "text-purple-500" },
  { value: 4, label: "good", color: "text-green-500" },
  { value: 5, label: "rad", color: "text-orange-500" },
]

/** The level an average rounds to, for its face. */
export function satisfactionLevel(value: number) {
  const index = Math.min(5, Math.max(1, Math.round(value))) - 1
  return SATISFACTION_LEVELS[index]!
}

const EYES = {
  dots: (
    <>
      <circle cx="8.6" cy="9.6" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="15.4" cy="9.6" r="1.15" fill="currentColor" stroke="none" />
    </>
  ),
  closed: <path d="M7 9.6h3.2M13.8 9.6H17" />,
  crossed: <path d="m7.2 8 2.8 2.8m0-2.8L7.2 10.8M14 8l2.8 2.8m0-2.8L14 10.8" />,
}

const FACES: Record<Satisfaction, { eyes: keyof typeof EYES; mouth: string }> = {
  1: { eyes: "crossed", mouth: "M8.2 16.6c1.1-1.2 2.4-1.8 3.8-1.8s2.7.6 3.8 1.8M12.6 15v2.4" },
  2: { eyes: "dots", mouth: "M8.4 16.8c1-1.3 2.2-1.9 3.6-1.9s2.6.6 3.6 1.9" },
  3: { eyes: "dots", mouth: "M8.4 15.6h7.2" },
  4: { eyes: "dots", mouth: "M7.8 13.6h8.4a4.2 4.2 0 0 1-8.4 0Z" },
  5: { eyes: "closed", mouth: "M6.8 13h10.4a5.2 5.2 0 0 1-10.4 0ZM8.4 15.4h7.2" },
}

/** A line-drawn face for a rating, in its color. */
export function SatisfactionFace({
  value,
  className,
}: {
  value: Satisfaction
  className?: string
}) {
  const face = FACES[value]
  const level = SATISFACTION_LEVELS[value - 1]!
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cn("size-6 shrink-0", level.color, className)}
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="10" />
      {EYES[face.eyes]}
      <path d={face.mouth} />
    </svg>
  )
}

/** A seat's rating, or an average with its number, as a face; nothing when unrated. */
export function SatisfactionBadge({
  value,
  average = false,
  ratings,
  className,
}: {
  value: number | null | undefined
  /** Shows the number beside the face. */
  average?: boolean
  /** How many ratings an average is over. */
  ratings?: number
  className?: string
}) {
  if (value === null || value === undefined) return null
  const level = satisfactionLevel(value)
  const title = average
    ? `Average ${value.toFixed(1)} of 5 (${level.label})${ratings ? ` from ${ratings} ${ratings === 1 ? "rating" : "ratings"}` : ""}`
    : `Rated ${level.label}`
  return (
    <span className={cn("inline-flex items-center gap-1", className)} title={title}>
      <SatisfactionFace value={level.value} className={average ? "size-5" : undefined} />
      {average ? (
        <span className="text-sm font-bold tabular-nums">
          <span className="sr-only">{title}</span>
          <span aria-hidden="true">{value.toFixed(1)}</span>
        </span>
      ) : (
        <span className="sr-only">{title}</span>
      )}
    </span>
  )
}

/** The five faces, worst to best; one click rates. */
export function SatisfactionPicker({
  value,
  onRate,
  disabled,
  size = "md",
  className,
}: {
  value?: number | null
  onRate: (value: Satisfaction) => void
  disabled?: boolean
  size?: "sm" | "md"
  className?: string
}) {
  return (
    <div role="group" aria-label="Rate this game" className={cn("flex gap-1", className)}>
      {SATISFACTION_LEVELS.map((level) => {
        const chosen = value === level.value
        return (
          <button
            key={level.value}
            type="button"
            disabled={disabled}
            aria-pressed={value ? chosen : undefined}
            aria-label={level.label}
            title={level.label}
            onClick={() => onRate(level.value)}
            className={cn(
              "flex flex-col items-center gap-0.5 rounded-lg transition hover:scale-110 focus-visible:scale-110 disabled:opacity-50",
              size === "md" ? "min-w-11 px-1 py-1" : "p-0.5",
              value && !chosen && "opacity-40 hover:opacity-100",
            )}
          >
            <SatisfactionFace value={level.value} className={size === "md" ? "size-8" : "size-6"} />
            {size === "md" && (
              <span className={cn("text-xs font-semibold", level.color)}>{level.label}</span>
            )}
          </button>
        )
      })}
    </div>
  )
}

/** A small floating card that asks once and gets out of the way: one click rates, the cross
 * dismisses it. */
export function SatisfactionPrompt({
  title = "How was the game?",
  onRate,
  onDismiss,
  className,
}: {
  title?: string
  onRate: (value: Satisfaction) => void
  onDismiss: () => void
  className?: string
}) {
  return (
    <div
      role="dialog"
      aria-label={title}
      className={cn(
        "bg-base-100 text-base-content border-base-300 fixed bottom-4 left-1/2 z-50 -translate-x-1/2 rounded-2xl border px-4 pt-2 pb-3 shadow-2xl",
        className,
      )}
    >
      <div className="mb-1 flex items-center justify-between gap-4">
        <p className="text-sm font-bold">{title}</p>
        <button
          type="button"
          className="btn btn-ghost btn-xs btn-circle -mr-2"
          aria-label="Not now"
          title="Not now"
          onClick={onDismiss}
        >
          <X className="size-4" />
        </button>
      </div>
      <SatisfactionPicker onRate={onRate} />
    </div>
  )
}

/** Rates the signed-in member's own seat. */
export const rateGame = (gameId: number | string, satisfaction: Satisfaction) =>
  api<{ data: Game }>(`/api/games/${gameId}/satisfaction`, {
    method: "PUT",
    body: JSON.stringify({ satisfaction }),
  }).then((body) => body.data)

/** Rates the signed-in member's seat and refreshes the game and the statistics. */
export function useRateGame(gameId: number) {
  const queryClient = useQueryClient()
  const { toast } = useToast()
  return useMutation({
    mutationFn: (value: Satisfaction) => rateGame(gameId, value),
    onSuccess: (game) => {
      queryClient.setQueryData(["games", String(gameId)], game)
      void invalidateGameRelated(queryClient)
    },
    onError: () => toast({ message: "Could not save your rating. Try again.", tone: "error" }),
  })
}

const PENDING_KEY = "the-gathering:rate-game"

/** Asks this browser to rate a game on its page, which may be a full page load away. */
export function requestGameRating(gameId: number) {
  try {
    sessionStorage.setItem(PENDING_KEY, String(gameId))
  } catch {
    // Storage is unavailable; the game page simply does not ask.
  }
}

/** Forgets a request to rate a game. */
export function clearGameRatingRequest() {
  try {
    sessionStorage.removeItem(PENDING_KEY)
  } catch {
    // Nothing to forget.
  }
}

/** Whether this browser was asked to rate the game; clearing forgets the request. */
export function useGameRatingRequest(gameId: number) {
  const [requested, setRequested] = useState(() => {
    try {
      return sessionStorage.getItem(PENDING_KEY) === String(gameId)
    } catch {
      return false
    }
  })
  const clear = () => {
    clearGameRatingRequest()
    setRequested(false)
  }
  return [requested, clear] as const
}
