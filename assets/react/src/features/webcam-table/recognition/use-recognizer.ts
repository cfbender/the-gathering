import { useCallback, useEffect, useRef, useState } from "react"
import { api, ApiError } from "@/lib/api"
import type { BundleInfo, Identification, WorkerRequest, WorkerResponse } from "./messages"
import type { GalleryArt, Quad, RgbaImage } from "./pipeline"

export type RecognizerState =
  | { status: "idle" }
  | { status: "checking" }
  /** No bundle has been published to the server yet (`/api/cardid/bundle` → 404). */
  | { status: "unavailable" }
  | { status: "loading"; version: string }
  /** `threads`: WASM threads the recognizer runs on (several only on an isolated page). */
  | { status: "ready"; version: string; arts: number; loadMs: number; threads: number }
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
 *
 * Loading starts as soon as `preload` turns true (the page passes "connected to the room"),
 * so the first click does not pay for the bundle download; a click or search before that
 * still starts it on demand.
 */
export function useRecognizer(preload = false) {
  const [state, setState] = useState<RecognizerState>({ status: "idle" })
  const workerRef = useRef<Worker | null>(null)
  const pendingRef = useRef(new Map<number, Pending>())
  const nextIdRef = useRef(0)
  const loadingRef = useRef<Promise<void> | null>(null)
  const rejectLoadRef = useRef<(error: Error) => void>(() => {})

  const start = useCallback(() => {
    if (loadingRef.current) return loadingRef.current
    setState({ status: "checking" })
    let resolveLoad!: () => void
    loadingRef.current = new Promise<void>((resolve, reject) => {
      resolveLoad = resolve
      rejectLoadRef.current = reject
    })
    const pending = pendingRef.current

    // `threads` 0 lets onnxruntime pick several; only the cross-origin-isolated table page can
    // share memory with WASM threads, anywhere else the worker gets one.
    const launch = (threads: number) => {
      let worker: Worker
      try {
        worker = new Worker(new URL("./recognizer.worker.ts", import.meta.url), { type: "module" })
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        setState({ status: "failed", message })
        rejectLoadRef.current(new Error(message))
        return
      }
      workerRef.current = worker
      let ready = false

      // onnxruntime cannot initialize twice in one worker, so a threaded start that fails gets
      // a fresh worker on one thread instead of a dead recognizer.
      const retryOnOneThread = (message: string) => {
        if (ready || threads === 1 || workerRef.current !== worker) return false
        console.warn(
          `Recognizer failed to start with ${threads || "auto"} threads; retrying with 1:`,
          message,
        )
        worker.terminate()
        launch(1)
        return true
      }

      worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
        const message = event.data
        if (message.type === "ready") {
          ready = true
          setState({
            status: "ready",
            version: message.version,
            arts: message.arts,
            loadMs: message.ms,
            threads: message.threads,
          })
          resolveLoad()
        } else if (message.type === "load_failed") {
          if (retryOnOneThread(message.message)) return
          setState({ status: "failed", message: message.message })
          rejectLoadRef.current(new Error(message.message))
        } else if (message.type === "identified" || message.type === "matches") {
          settle(pending, message.id)?.resolve(
            message.type === "identified" ? message.result : message.arts,
          )
        } else {
          settle(pending, message.id)?.reject(new Error(message.message))
        }
      }
      worker.onerror = (event) => {
        const message = event.message || "worker crashed"
        if (retryOnOneThread(message)) return
        setState({ status: "failed", message })
        rejectLoadRef.current(new Error(message))
        for (const id of pending.keys()) settle(pending, id)?.reject(new Error("worker crashed"))
      }

      api<{ data: BundleInfo }>("/api/cardid/bundle")
        .then(({ data }) => {
          if (workerRef.current !== worker) return
          setState({ status: "loading", version: data.version })
          post(worker, { type: "load", bundle: data, threads })
        })
        .catch((error: unknown) => {
          if (workerRef.current !== worker) return
          rejectLoadRef.current(error instanceof Error ? error : new Error(String(error)))
          if (error instanceof ApiError && error.status === 404) setState({ status: "unavailable" })
          else
            setState({
              status: "failed",
              message: error instanceof Error ? error.message : String(error),
            })
        })
    }

    launch(window.crossOriginIsolated ? 0 : 1)
    return loadingRef.current
  }, [])

  useEffect(() => {
    // Failures are already reflected in `state`; nothing awaits this warmup.
    if (preload) start().catch(() => {})
  }, [preload, start])

  useEffect(() => {
    const pending = pendingRef.current
    return () => {
      workerRef.current?.terminate()
      workerRef.current = null
      rejectLoadRef.current(new Error("table closed"))
      loadingRef.current = null
      for (const id of pending.keys()) settle(pending, id)?.reject(new Error("table closed"))
    }
  }, [])

  const request = useCallback(
    async <T>(
      build: (id: number) => WorkerRequest,
      transfer: Transferable[],
      timeoutMs?: number,
    ) => {
      await start()
      return new Promise<T>((resolve, reject) => {
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
      })
    },
    [start],
  )

  /** Identifies the card under the click, or inside `quad` when the user drew its corners. */
  const identify = useCallback(
    (image: RgbaImage, x: number, y: number, timeoutMs = IDENTIFY_TIMEOUT_MS, quad?: Quad) => {
      const rgba = image.data.buffer.slice(0) as ArrayBuffer
      return request<Identification>(
        (id) => ({
          type: "identify",
          id,
          rgba,
          width: image.width,
          height: image.height,
          x,
          y,
          ...(quad ? { quad } : {}),
        }),
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

  const printings = useCallback(
    (artId: string) => request<GalleryArt[]>((id) => ({ type: "printings", id, artId }), []),
    [request],
  )

  const locate = useCallback(
    (printingIds: string[]) =>
      request<GalleryArt[]>((id) => ({ type: "locate", id, printingIds }), []),
    [request],
  )

  return { state, ready: state.status === "ready", identify, search, printings, locate }
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
