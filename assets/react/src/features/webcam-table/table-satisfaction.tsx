import { useState } from "react"
import { isGameFormat, type TableMode } from "@/features/games/game-format"
import {
  SatisfactionPrompt,
  clearGameRatingRequest,
  useRateGame,
} from "@/features/satisfaction/satisfaction"
import type { TableParticipant } from "./room-types"

/** Asks this seat how the game went: once it is knocked out, rated with the game when it is
 * recorded, and again for a recorded game it never rated (shown here after a rematch; a closing
 * table asks on the game's page instead). Mount it per game so a dismissal lasts one game. */
export function TableSatisfactionPrompt({
  participant,
  spectating,
  mode,
  unratedGame,
  onRate,
  onClearUnratedGame,
}: {
  participant: TableParticipant
  spectating: boolean
  mode: TableMode
  unratedGame: number | null
  /** Rates the game in progress. */
  onRate: (satisfaction: number) => void
  onClearUnratedGame: () => void
}) {
  const [closed, setClosed] = useState(false)
  if (unratedGame !== null)
    return <RecordedGamePrompt gameId={unratedGame} onDone={onClearUnratedGame} />
  if (
    closed ||
    spectating ||
    !isGameFormat(mode) ||
    !participant.eliminated ||
    participant.satisfaction
  )
    return null
  return (
    <SatisfactionPrompt
      title="You're out. How was the game?"
      onRate={(value) => {
        onRate(value)
        setClosed(true)
      }}
      onDismiss={() => setClosed(true)}
    />
  )
}

/** Rates a game already in history, from the seat's own account. */
function RecordedGamePrompt({ gameId, onDone }: { gameId: number; onDone: () => void }) {
  const rate = useRateGame(gameId)
  const done = () => {
    clearGameRatingRequest()
    onDone()
  }
  return (
    <SatisfactionPrompt
      title="How was the last game?"
      onRate={(value) => {
        rate.mutate(value)
        done()
      }}
      onDismiss={done}
    />
  )
}
