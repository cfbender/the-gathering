import { createContext, useContext } from "react"

/** Follows the drawn size of an element showing `stream`; returns the unwatch. The room uses
 * the sizes to ask the server for each board's simulcast layer. */
export type WatchTile = (stream: MediaStream, element: Element) => () => void

export const WatchTileContext = createContext<WatchTile>(() => () => {})

export function useWatchTile() {
  return useContext(WatchTileContext)
}
