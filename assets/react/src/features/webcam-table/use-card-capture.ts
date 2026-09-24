import { useCallback, useEffect, useRef, useState } from "react"
import type { CaptureRequest, CaptureResponse } from "./data-messages"
import { canViewBoard } from "./media-policy"
import { liveStatus, type RoomLink } from "./room-link"
import type { CapturedCard } from "./room-types"
import type { LocalCamera } from "./use-local-camera"
import type { PeerConnections } from "./use-peer-connections"

/** How long a clicker waits for a remote camera's crop before giving up on it. */
export const CAPTURE_TIMEOUT_MS = 8000

interface PendingCapture {
  targetPeerId: string
  inspect: boolean
  timeout: number
}

/** Native camera crops for card identification. Your own board is cropped locally; another
 * board's owner is asked over the data channel and answers with a crop of their camera. */
export function useCardCapture(
  link: RoomLink,
  playerId: number,
  { crop, videoEnabled }: Pick<LocalCamera, "crop" | "videoEnabled">,
  { send, listen, revealTarget }: Pick<PeerConnections, "send" | "listen" | "revealTarget">,
  setStatus: (status: string) => void,
) {
  const pendingRef = useRef(new Map<string, PendingCapture>())
  const [capture, setCapture] = useState<CapturedCard | null>(null)

  /** Forgets outstanding requests to one peer, or to everyone when `peerId` is null. */
  const cancel = useCallback(
    (peerId: string | null, announce: boolean) => {
      let cancelled = false
      for (const [requestId, pending] of pendingRef.current) {
        if (peerId !== null && pending.targetPeerId !== peerId) continue
        window.clearTimeout(pending.timeout)
        pendingRef.current.delete(requestId)
        cancelled = true
      }
      if (cancelled && announce) setStatus(liveStatus(link.spectator))
    },
    [link, setStatus],
  )

  const answer = useCallback(
    (fromPeerId: string, request: CaptureRequest) => {
      if (!canViewBoard(link.peerId, fromPeerId, revealTarget()) || !videoEnabled()) return
      const result = crop(request.x, request.y)
      if (!result) return
      const response: CaptureResponse = {
        type: "capture_response",
        requestId: request.requestId,
        private: !!revealTarget(),
        ...result,
      }
      // A closed channel or an oversized crop is dropped; the clicker's request times out.
      send(fromPeerId, response)
    },
    [crop, link, revealTarget, send, videoEnabled],
  )

  const receive = useCallback(
    (fromPeerId: string, response: CaptureResponse) => {
      const pending = pendingRef.current.get(response.requestId)
      if (pending?.targetPeerId !== fromPeerId) return
      window.clearTimeout(pending.timeout)
      pendingRef.current.delete(response.requestId)
      setStatus(liveStatus(link.spectator))
      const owner = link.participants.find((item) => item.peer_id === fromPeerId)
      if (!owner || owner.camera_off || !canViewBoard(fromPeerId, link.peerId, owner.reveal_to))
        return
      const { type: _type, requestId: _requestId, ...image } = response
      setCapture({
        peerId: fromPeerId,
        playerId: owner.player_id,
        inspect: pending.inspect,
        ...image,
      })
    },
    [link, setStatus],
  )

  useEffect(
    () =>
      listen({
        message(fromPeerId, message) {
          if (message.type === "capture_request") answer(fromPeerId, message)
          else receive(fromPeerId, message)
        },
        left: (peerId) => cancel(peerId, true),
      }),
    [answer, cancel, listen, receive],
  )

  useEffect(() => () => cancel(null, false), [cancel])

  const requestCapture = useCallback(
    (targetPeerId: string, x: number, y: number, inspect = false) => {
      const owner = link.participants.find((item) => item.peer_id === targetPeerId)
      if (!owner || owner.camera_off || !canViewBoard(targetPeerId, link.peerId, owner.reveal_to))
        return
      if (targetPeerId === link.peerId) {
        const result = crop(x, y)
        if (result)
          setCapture({
            peerId: targetPeerId,
            playerId,
            inspect,
            private: !!revealTarget(),
            ...result,
          })
        return
      }
      const requestId = crypto.randomUUID()
      const timeout = window.setTimeout(() => {
        if (!pendingRef.current.delete(requestId)) return
        setStatus(`${owner.player_name}'s camera did not send a crop; click the card again`)
      }, CAPTURE_TIMEOUT_MS)
      pendingRef.current.set(requestId, { targetPeerId, inspect, timeout })
      if (!send(targetPeerId, { type: "capture_request", requestId, x, y })) {
        window.clearTimeout(timeout)
        pendingRef.current.delete(requestId)
        setStatus(`${owner.player_name}'s camera is still connecting; click again in a moment`)
        return
      }
      setStatus("Requesting native camera crop…")
    },
    [cancel, crop, link, playerId, revealTarget, send, setStatus],
  )

  const dismissCapture = useCallback(() => setCapture(null), [])
  const cancelAll = useCallback(() => cancel(null, false), [cancel])

  return { capture, requestCapture, dismissCapture, cancelAll }
}
