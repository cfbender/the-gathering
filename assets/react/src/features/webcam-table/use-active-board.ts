import { useEffect, useRef, useState } from "react"
import type { TableParticipant } from "./room-types"

/** Which board fills the stage: the pinned/selected one, else the newest remote joiner, else you.
 * Falls back to your own board when the selected player leaves. */
export function useActiveBoard(participants: TableParticipant[], localPeerId: string) {
  const [selectedPeerId, setSelectedPeerId] = useState(localPeerId)
  const [pinned, setPinned] = useState(false)
  const knownPeers = useRef(new Set<string>([localPeerId]))

  useEffect(() => {
    const present = new Set(participants.map((participant) => participant.peer_id))
    const newcomers = participants.filter(
      (participant) => !knownPeers.current.has(participant.peer_id),
    )
    knownPeers.current = new Set([localPeerId, ...present])

    if (!present.has(selectedPeerId) && selectedPeerId !== localPeerId) {
      setSelectedPeerId(localPeerId)
      setPinned(false)
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
      setPinned(true)
    },
    togglePin: () => setPinned((value) => !value),
  }
}
