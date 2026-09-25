import type { VideoFlip } from "./board"
import type { CapturedCard } from "./room-types"

type Crop = Pick<CapturedCard, "image" | "cropSize" | "clickX" | "clickY">

/** Mirrors a native crop the way the viewer flips that board, so recognition, the picker, and
 * correction uploads all see the card as it appears on screen. Unflipped crops pass through. */
export async function orientCrop<T extends Crop>(crop: T, flip: VideoFlip): Promise<T> {
  if (!flip.horizontal && !flip.vertical) return crop
  const size = crop.cropSize
  const image = new Image()
  image.src = crop.image
  await image.decode()
  const canvas = document.createElement("canvas")
  canvas.width = size
  canvas.height = size
  const context = canvas.getContext("2d")
  if (!context) throw new Error("2d canvas unavailable")
  context.setTransform(
    flip.horizontal ? -1 : 1,
    0,
    0,
    flip.vertical ? -1 : 1,
    flip.horizontal ? size : 0,
    flip.vertical ? size : 0,
  )
  context.drawImage(image, 0, 0, size, size)
  return {
    ...crop,
    image: canvas.toDataURL("image/jpeg", 0.92),
    clickX: flip.horizontal ? size - crop.clickX : crop.clickX,
    clickY: flip.vertical ? size - crop.clickY : crop.clickY,
  }
}
