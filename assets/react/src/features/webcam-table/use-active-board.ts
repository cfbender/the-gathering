import { useEffect, useRef, useState } from "react"
import type { TableParticipant } from "./room-types"
import type { ViewMode } from "./table-preferences"

/** Which board the viewer picked. Clicking a player soft-pins their board: it overrides following
 * the turn, or fills the stage instead of the grid, until released. The pin remembers the view
 * mode it was made in, so switching modes starts unpinned. Unpinned, the selection tracks the
 * newest remote joiner, and it falls back to your own board when the selected player leaves. */
export function useActiveBoard(
  participants: TableParticipant[],
  localPeerId: string,
  viewMode: ViewMode,
) {
  const [selectedPeerId, setSelectedPeerId] = useState(localPeerId)
  const [pinnedIn, setPinnedIn] = useState<ViewMode | null>(null)
  const pinned = pinnedIn === viewMode
  const knownPeers = useRef(new Set<string>([localPeerId]))

  useEffect(() => {
    const present = new Set(participants.map((participant) => participant.peer_id))
    const newcomers = participants.filter(
      (participant) => !knownPeers.current.has(participant.peer_id),
    )
    knownPeers.current = new Set([localPeerId, ...present])

    if (!present.has(selectedPeerId) && selectedPeerId !== localPeerId) {
      setSelectedPeerId(localPeerId)
      setPinnedIn(null)
    } else if (!pinned && newcomers.length > 0) {
      const newest = newcomers[newcomers.length - 1]
      if (newest && newest.peer_id !== localPeerId) setSelectedPeerId(newest.peer_id)
    }
  }, [localPeerId, participants, pinned, selectedPeerId])

  return {
    selectedPeerId,
    pinned,
    select: (peerId: string) => {
      setSelectedPeerId(peerId)
      setPinnedIn(viewMode)
    },
    release: () => setPinnedIn(null),
  }
}
