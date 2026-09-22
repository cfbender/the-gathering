import { useCallback, useEffect, useRef, useState } from "react"
import { api, ApiError } from "@/lib/api"
import type { BundleInfo, Identification, WorkerRequest, WorkerResponse } from "./messages"
import type { GalleryArt, RgbaImage } from "./pipeline"

export type RecognizerState =
  | { status: "checking" }
  /** No bundle has been published to the server yet (`/api/cardid/bundle` → 404). */
  | { status: "unavailable" }
  | { status: "loading"; version: string }
  | { status: "ready"; version: string; arts: number; loadMs: number }
  | { status: "failed"; message: string }

/** Clicks wait at most this long for a result before the UI falls back to deck suggestions. */
export const IDENTIFY_TIMEOUT_MS = 2000

interface Pending {
  resolve: (value: unknown) => void
  reject: (reason: Error) => void
  timer?: number
}

/**
 * Owns the recognition Web Worker for the lifetime of a table: fetches the bundle
 * description once, lets the worker download and warm the graphs, and exposes promise-based
 * `identify` and `search` calls. Everything heavy happens off the main thread.
 */
export function useRecognizer() {
  const [state, setState] = useState<RecognizerState>({ status: "checking" })
  const workerRef = useRef<Worker | null>(null)
  const pendingRef = useRef(new Map<number, Pending>())
  const nextIdRef = useRef(0)

  useEffect(() => {
    let disposed = false
    const worker = new Worker(new URL("./recognizer.worker.ts", import.meta.url), {
      type: "module",
    })
    workerRef.current = worker
    const pending = pendingRef.current

    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const message = event.data
      if (message.type === "ready") {
        setState({
          status: "ready",
          version: message.version,
          arts: message.arts,
          loadMs: message.ms,
        })
      } else if (message.type === "load_failed") {
        setState({ status: "failed", message: message.message })
      } else if (message.type === "identified" || message.type === "matches") {
        settle(pending, message.id)?.resolve(
          message.type === "identified" ? message.result : message.arts,
        )
      } else {
        settle(pending, message.id)?.reject(new Error(message.message))
      }
    }
    worker.onerror = (event) =>
      setState({ status: "failed", message: event.message || "worker crashed" })

    api<{ data: BundleInfo }>("/api/cardid/bundle")
      .then(({ data }) => {
        if (disposed) return
        setState({ status: "loading", version: data.version })
        post(worker, { type: "load", bundle: data })
      })
      .catch((error: unknown) => {
        if (disposed) return
        if (error instanceof ApiError && error.status === 404) setState({ status: "unavailable" })
        else
          setState({
            status: "failed",
            message: error instanceof Error ? error.message : String(error),
          })
      })

    return () => {
      disposed = true
      worker.terminate()
      workerRef.current = null
      for (const entry of pending.values()) entry.reject(new Error("table closed"))
      pending.clear()
    }
  }, [])

  const request = useCallback(
    <T>(build: (id: number) => WorkerRequest, transfer: Transferable[], timeoutMs?: number) =>
      new Promise<T>((resolve, reject) => {
        const worker = workerRef.current
        if (!worker) return reject(new Error("recognizer not running"))
        const id = (nextIdRef.current += 1)
        const entry: Pending = { resolve: resolve as (value: unknown) => void, reject }
        if (timeoutMs) {
          entry.timer = window.setTimeout(() => {
            pendingRef.current.delete(id)
            reject(new Error(`no result within ${timeoutMs} ms`))
          }, timeoutMs)
        }
        pendingRef.current.set(id, entry)
        worker.postMessage(build(id), transfer)
      }),
    [],
  )

  const identify = useCallback(
    (image: RgbaImage, x: number, y: number, timeoutMs = IDENTIFY_TIMEOUT_MS) => {
      const rgba = image.data.buffer.slice(0) as ArrayBuffer
      return request<Identification>(
        (id) => ({ type: "identify", id, rgba, width: image.width, height: image.height, x, y }),
        [rgba],
        timeoutMs,
      )
    },
    [request],
  )

  const search = useCallback(
    (query: string) => request<GalleryArt[]>((id) => ({ type: "search", id, query }), []),
    [request],
  )

  return { state, ready: state.status === "ready", identify, search }
}

function post(worker: Worker, message: WorkerRequest) {
  worker.postMessage(message)
}

function settle(pending: Map<number, Pending>, id: number): Pending | undefined {
  const entry = pending.get(id)
  if (!entry) return undefined
  pending.delete(id)
  if (entry.timer) window.clearTimeout(entry.timer)
  return entry
}

/** Decodes a data-URL image (the camera owner's JPEG crop) into RGBA pixels. */
export async function decodeImage(dataUrl: string): Promise<RgbaImage> {
  const blob = await (await fetch(dataUrl)).blob()
  const bitmap = await createImageBitmap(blob)
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height)
  const context = canvas.getContext("2d")
  if (!context) throw new Error("2d canvas unavailable")
  context.drawImage(bitmap, 0, 0)
  const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height)
  bitmap.close()
  return { data: pixels.data, width: pixels.width, height: pixels.height }
}
