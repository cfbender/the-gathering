import { useCallback, useEffect, useRef, useState } from "react"
import type { DeckSummary } from "@/features/decks/decks"
import { isClear, type Recognition } from "./card-suggestions"
import type { GalleryArt } from "./recognition/pipeline"
import { decodeImage, useRecognizer } from "./recognition/use-recognizer"
import type { BoardCard, CapturedCard, IdentifiedCard, TableParticipant } from "./room-types"
import type { WebcamRoom } from "./table-view"
import type { useCorrectionUpload } from "./use-correction-upload"

/** What the card overlay on the active board is showing: an identified board entry (offering
 * "Wrong card?" while its capture is still current), or a printing from the gallery search. */
export type Preview =
  | { kind: "entry"; entry: BoardCard; shown: IdentifiedCard; correctable: boolean }
  | { kind: "art"; card: IdentifiedCard }

export function toCard(art: GalleryArt): IdentifiedCard {
  return { id: art.id, name: art.name, set: art.set, collector_number: art.collector_number }
}

/** Runs the recognizer on every new capture: decode the owner's crop, identify at the click,
 * and hold the outcome next to the capture it belongs to. The outcome is only reported while
 * that same capture is current, so a new click never sees the previous click's answer.
 * The bundle warms up in the background once `connected`, not on the first click. */
function useRecognition(capture: CapturedCard | null, connected: boolean) {
  const recognizer = useRecognizer(connected)
  const [outcome, setOutcome] = useState<{ capture: CapturedCard; recognition: Recognition }>()

  useEffect(() => {
    if (!capture) return
    let stale = false
    decodeImage(capture.image)
      .then((image) => recognizer.identify(image, capture.clickX, capture.clickY))
      .then((result) => {
        if (!stale) setOutcome({ capture, recognition: { status: "done", result } })
      })
      .catch((error: unknown) => {
        if (stale) return
        const message = error instanceof Error ? error.message : String(error)
        setOutcome({
          capture,
          recognition: {
            status: "skipped",
            reason: message.startsWith("no result") ? "timed out" : message,
          },
        })
      })
    return () => {
      stale = true
    }
  }, [capture, recognizer.identify])

  const recognition: Recognition =
    outcome && outcome.capture === capture
      ? outcome.recognition
      : { status: "identifying", loading: !recognizer.ready }
  return { recognizer, recognition }
}

interface Options {
  room: WebcamRoom
  seated: TableParticipant[]
  decks: DeckSummary[]
  playerName: string
  corrections: ReturnType<typeof useCorrectionUpload>
  /** Another overlay (help, end game) owns the screen; keep the picker closed meanwhile. */
  blocked: boolean
}

/** From a click's capture to a named card: recognize it, record a clear answer, or open the
 * picker when a human has to choose, then show the result and offer to correct it. */
export function useCardIdentificationFlow({
  room,
  seated,
  decks,
  playerName,
  corrections,
  blocked,
}: Options) {
  // Presence has synced once any seat is listed, i.e. the channel join succeeded.
  const { recognizer, recognition } = useRecognition(room.capture, room.participants.length > 0)
  const [preview, setPreview] = useState<Preview | null>(null)
  /** The picker is open by request ("Wrong card?"), replacing this entry if one is named. */
  const [picker, setPicker] = useState<{ replacing: string | null } | null>(null)
  const autoChosen = useRef<CapturedCard | null>(null)

  const captureOwner = room.capture
    ? seated.find((participant) => participant.peer_id === room.capture?.peerId)
    : undefined
  // Only a seat's owner may pick its commander, so deck shortcuts appear on your own clicks only.
  const captureIsLocal = captureOwner?.peer_id === room.peerId
  const ownerDecks = captureOwner
    ? decks.filter((deck) => deck.player_id === captureOwner.player_id)
    : []
  const suggestions = captureIsLocal ? ownerDecks.slice(0, 5) : []
  const candidates = recognition.status === "done" ? recognition.result.candidates : []
  // The picker is for the cases a human has to settle: no recognizer, a near-tie, a
  // Shift+click asking to choose, or "Wrong card?" on a result. A clear answer to a plain
  // click is recorded without it and shown as the card itself.
  const needsChoice =
    room.capture !== null &&
    (room.capture.inspect ||
      recognition.status === "skipped" ||
      (recognition.status === "done" && !isClear(candidates)))
  const pickerOpen =
    room.capture !== null &&
    captureOwner !== undefined &&
    !preview &&
    !blocked &&
    (needsChoice || !!picker)

  const dismissPicker = useCallback(() => {
    setPicker(null)
    room.dismissCapture()
  }, [room])

  const closePreview = useCallback(() => {
    setPreview(null)
    room.dismissCapture()
  }, [room])

  const chooseDeckForCapture = useCallback(
    (deckId: number) => {
      if (!captureIsLocal) return
      room.chooseDeck(deckId)
      dismissPicker()
    },
    [captureIsLocal, dismissPicker, room],
  )

  /** Adds the card to the owner's board list at every seat and shows it; a card that is one of
   * the owner's commanders also picks that deck when they have not chosen one yet. Replaces the
   * entry being corrected when the picker came from "Wrong card?". */
  const chooseCard = useCallback(
    (art: GalleryArt, explicit = true) => {
      if (!captureOwner) return
      corrections.save(
        room.capture,
        recognition.status === "done" ? recognition.result : undefined,
        art.id,
        "version" in recognizer.state ? recognizer.state.version : "unavailable",
        explicit,
      )
      const commanderDeck = decks.find(
        (deck) =>
          deck.player_id === captureOwner.player_id &&
          deck.commander_name.toLowerCase() === art.name.toLowerCase(),
      )
      if (commanderDeck && !captureOwner.deck_id && captureOwner.peer_id === room.peerId)
        room.chooseDeck(commanderDeck.id)
      if (picker?.replacing) room.removeCard(picker.replacing)
      const entry = room.announceCard(captureOwner.peer_id, playerName, toCard(art))
      setPicker(null)
      setPreview({ kind: "entry", entry, shown: toCard(art), correctable: true })
    },
    [captureOwner, corrections, decks, picker, playerName, recognition, recognizer.state, room],
  )

  // A new click replaces whatever the last one left on screen.
  useEffect(() => {
    setPreview(null)
    setPicker(null)
  }, [room.capture])

  useEffect(() => {
    const top = candidates[0]
    if (!top || !room.capture || room.capture.inspect || !isClear(candidates)) return
    if (autoChosen.current === room.capture) return
    autoChosen.current = room.capture
    chooseCard(top, false)
  }, [candidates, chooseCard, room.capture])

  useEffect(() => {
    function choose(event: KeyboardEvent) {
      if (!pickerOpen || event.key < "1" || event.key > "5") return
      if (event.target instanceof HTMLElement && event.target.matches("input, textarea, select"))
        return
      const index = Number(event.key) - 1
      const art = candidates[index]
      if (art) return chooseCard(art)
      const deck = suggestions[index]
      if (deck && recognition.status === "skipped") chooseDeckForCapture(deck.id)
    }
    window.addEventListener("keydown", choose)
    return () => window.removeEventListener("keydown", choose)
  }, [candidates, chooseCard, chooseDeckForCapture, pickerOpen, recognition.status, suggestions])

  return {
    recognizer,
    recognition,
    capture: room.capture,
    captureOwner,
    suggestions,
    pickerOpen,
    preview,
    chooseCard,
    chooseDeckForCapture,
    dismissPicker,
    closePreview,
    previewEntry: (entry: BoardCard) =>
      setPreview({ kind: "entry", entry, shown: entry.card, correctable: false }),
    previewArt: (art: GalleryArt) => setPreview({ kind: "art", card: toCard(art) }),
    /** "Wrong card?" on the previewed entry: reopen the picker to replace it. */
    correctPreview: (entryId: string) => {
      setPicker({ replacing: entryId })
      setPreview(null)
    },
  }
}

export type CardIdentificationFlow = ReturnType<typeof useCardIdentificationFlow>
