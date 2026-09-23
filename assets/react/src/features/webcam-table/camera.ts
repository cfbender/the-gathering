import { useEffect, useState } from "react"

export function openCamera(deviceId: string) {
  return navigator.mediaDevices.getUserMedia({
    video: {
      ...(deviceId ? { deviceId: { exact: deviceId } } : {}),
      width: { ideal: 1920 },
      height: { ideal: 1080 },
      frameRate: { ideal: 30 },
    },
    audio: false,
  })
}

export function useCameraDevices(stream: MediaStream | null) {
  const [devices, setDevices] = useState<MediaDeviceInfo[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let disposed = false
    const update = async () => {
      try {
        const all = await navigator.mediaDevices.enumerateDevices()
        if (!disposed) {
          setDevices(all.filter((device) => device.kind === "videoinput"))
          setError(null)
        }
      } catch {
        if (!disposed) setError("Camera devices could not be listed. Check browser permissions.")
      }
    }
    void update()
    navigator.mediaDevices?.addEventListener("devicechange", update)
    return () => {
      disposed = true
      navigator.mediaDevices?.removeEventListener("devicechange", update)
    }
  }, [stream])
  return { devices, error }
}

export function videoHealth(stream: MediaStream | null): string {
  const track = stream?.getVideoTracks()[0]
  if (!track) return "No camera track is available. Check camera permissions."
  const { width, height, frameRate } = track.getSettings()
  return `${track.label || "Camera"}: ${width ?? "?"} × ${height ?? "?"}, ${frameRate?.toFixed(0) ?? "?"} fps. Track ${track.readyState}; ${track.enabled ? "enabled" : "disabled"}${track.muted ? ", source muted" : ""}.`
}
