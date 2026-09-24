import type { GalleryArt } from "./recognition/pipeline"
import type { SeatCounters } from "./seat-counters"

export interface TableParticipant extends SeatCounters {
  peer_id: string
  player_id: number
  player_name: string
  life: number
  /** Server clock (ms) when the seat was taken; default seat order is join order. */
  joined_at: number
  camera_off: boolean
  reveal_to?: string | null
  eliminated: boolean
  spectator?: boolean
  /** Retained result seat after an eliminated player disconnects. */
  departed?: boolean
  deck_id?: number
  deck_name?: string
}

/** Status a player publishes about their own seat; mirrors the channel's `update_status`. */
export type SeatStatus = Partial<
  Pick<TableParticipant, "life" | "camera_off" | "eliminated"> & SeatCounters
>

export interface Monarch {
  peer_id: string
  player_name: string
}

export interface CapturedCard {
  peerId: string
  playerId: number
  /** JPEG data URL of the native crop around the click. */
  image: string
  nativeWidth: number
  nativeHeight: number
  /** Side of the square crop in native pixels (640 unless the camera is smaller). */
  cropSize: number
  /** The click in crop pixels; the crop is clamped to the frame so it is not always centred. */
  clickX: number
  clickY: number
  /** Camera owner's consent to share corrections, carried with the crop. */
  shareCorrections: boolean
  /** Shift+click: the clicker wants to see and choose among the candidates even when the
   * recognizer is sure. A plain click logs a clear answer silently. */
  inspect: boolean
  /** Keep reveal captures private even if identification finishes after the reveal ends. */
  private: boolean
}

/** A card a seat named on someone's board, recognized or picked by hand. */
export type IdentifiedCard = Pick<GalleryArt, "id" | "name" | "set" | "collector_number">

/** One entry in the shared per-board list of identified cards. The server owns the list: it
 * validates every change and broadcasts the whole list, including to seats that join later. */
export interface BoardCard {
  id: string
  ownerPeerId: string
  byPlayerName: string
  card: IdentifiedCard
  /** Clicker's clock (ms); only used to order the list. */
  at: number
}
