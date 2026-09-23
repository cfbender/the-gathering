import { useMutation } from "@tanstack/react-query"
import { useRef, useState } from "react"
import { api } from "@/lib/api"
import type { Identification } from "./recognition/messages"
import type { CapturedCard } from "./use-webcam-room"

const PREFERENCE = "the-gathering:share-card-corrections"

export function sharesCorrections() {
  return localStorage.getItem(PREFERENCE) !== "false"
}

/** Only explicit choices are labels. Never learn from the model's automatic answers, and never
 * upload a crop taken during a private reveal: that hand was shown to one player only. */
export function correctionPayload(
  captureId: string,
  capture: CapturedCard,
  result: Identification | undefined,
  label: string,
  version: string,
  explicit: boolean,
) {
  if (!explicit || capture.private || !sharesCorrections() || !capture.shareCorrections) {
    return null
  }
  const [top, second] = result?.candidates ?? []
  return {
    capture_id: captureId,
    image: capture.image,
    click: [capture.clickX, capture.clickY],
    quad: result?.quad ?? null,
    up_vote: result?.upVote ?? null,
    label,
    top1: top?.id ?? null,
    similarity: top?.score ?? null,
    margin: top && second ? top.score - second.score : null,
    bundle_version: version,
  }
}

async function boundedJpeg(image: string) {
  if (image.length <= 190_000) return image
  const bitmap = await createImageBitmap(await (await fetch(image)).blob())
  const canvas = document.createElement("canvas")
  canvas.width = bitmap.width
  canvas.height = bitmap.height
  canvas.getContext("2d")?.drawImage(bitmap, 0, 0)
  bitmap.close()
  for (const quality of [0.7, 0.5, 0.3]) {
    const jpeg = canvas.toDataURL("image/jpeg", quality)
    if (jpeg.length <= 190_000) return jpeg
  }
  throw new Error("Crop too large")
}

export function useCorrectionUpload() {
  const [enabled, setEnabled] = useState(sharesCorrections)
  const ids = useRef(new WeakMap<CapturedCard, string>())
  const mutation = useMutation({
    mutationFn: async (payload: NonNullable<ReturnType<typeof correctionPayload>>) => {
      const image = await boundedJpeg(payload.image)
      // Recheck after asynchronous encoding in case the player opted out meanwhile.
      if (!sharesCorrections()) return false
      await api("/api/cardid/corrections", {
        method: "POST",
        body: JSON.stringify({ ...payload, image }),
      })
      return true
    },
    retry: false,
    scope: { id: "card-corrections" },
  })

  return {
    enabled,
    setEnabled(value: boolean) {
      localStorage.setItem(PREFERENCE, String(value))
      setEnabled(value)
      mutation.reset()
    },
    status: mutation.isError
      ? "Correction not saved for training."
      : mutation.isSuccess && mutation.data
        ? "Correction saved for training."
        : "",
    save(
      capture: CapturedCard | null,
      result: Identification | undefined,
      label: string,
      version: string,
      explicit: boolean,
    ) {
      if (!capture) return
      const id = ids.current.get(capture) ?? crypto.randomUUID()
      ids.current.set(capture, id)
      const payload = correctionPayload(id, capture, result, label, version, explicit)
      if (payload) mutation.mutate(payload)
    },
  }
}

export function CorrectionPreference({
  upload,
}: {
  upload: ReturnType<typeof useCorrectionUpload>
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-1 text-[0.65rem] text-white/60">
      <label className="flex items-center gap-1.5">
        <input
          type="checkbox"
          className="checkbox checkbox-xs"
          checked={upload.enabled}
          onChange={(event) => upload.setEnabled(event.target.checked)}
        />
        Share card crops & picks for training
      </label>
      <span role="status">{upload.status}</span>
    </div>
  )
}
