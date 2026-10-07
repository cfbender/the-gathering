import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { RefreshCw } from "lucide-react"
import { useState } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { api, ApiError } from "@/lib/api"
import { errorMessage, isSudoRequired } from "@/lib/auth"

export interface SoftwareUpdateStatus {
  version: string | null
  channel: "release" | "nightly" | "preview" | null
  method: "systemd" | "watchtower" | null
  pending: boolean
  requested_at: string | null
  latest: { version: string; url: string } | null
  update_available: boolean | null
  check_error: string | null
}

const queryKey = ["admin", "software-update"]

/**
 * Shows the running version against the newest build on GitHub and hands an update to whatever
 * runs the server (systemd in the LXC, Watchtower for Docker). The server restarts partway
 * through, so the status is polled until it answers again and the version is compared.
 */
export function SoftwareUpdate() {
  const queryClient = useQueryClient()
  const [confirm, setConfirm] = useState(false)
  const [requestedFrom, setRequestedFrom] = useState<{ version: string | null } | null>(null)
  const status = useQuery({
    queryKey,
    queryFn: async () =>
      (await api<{ data: SoftwareUpdateStatus }>("/api/admin/software-update")).data,
    // Keep polling while an update is pending, and through the restart that follows it.
    refetchInterval: (query) =>
      query.state.data?.pending || (requestedFrom !== null && query.state.error) ? 5000 : false,
  })
  const update = useMutation({
    mutationFn: async () =>
      (await api<{ data: SoftwareUpdateStatus }>("/api/admin/software-update", { method: "POST" }))
        .data,
    onSuccess: (data) => {
      setRequestedFrom({ version: status.data?.version ?? null })
      queryClient.setQueryData(queryKey, data)
    },
  })
  const error = update.error ?? status.error
  const data = status.data
  const updating = data?.pending || (requestedFrom !== null && status.isError)
  const finished = requestedFrom !== null && !updating && data !== undefined

  return (
    <section
      className="card bg-base-200 border-base-300 border"
      aria-labelledby="software-update-heading"
    >
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="software-update-heading" className="flex items-center gap-2 text-lg font-semibold">
          <RefreshCw className="size-5" /> Software update
        </h2>
        {data && <VersionLine status={data} />}
        {data && <LatestLine status={data} />}
        <p className="text-base-content/70 text-sm">{methodDescription(data?.method)}</p>
        {updating && (
          <p role="status" className="text-sm">
            {status.isError
              ? "The server is restarting…"
              : `Update requested${requestedAt(data)}. The server restarts as soon as the new build is installed; this page keeps checking.`}
          </p>
        )}
        {finished && (
          <p role="status" className="text-sm">
            {data.version !== requestedFrom.version
              ? `Updated to ${data.version ?? "a development build"}.`
              : `The updater finished and the server is still on ${describeVersion(data.version)}. Check its logs if a newer build was expected.`}
          </p>
        )}
        <div>
          <button
            type="button"
            className="btn btn-primary"
            disabled={!data?.method || updating || update.isPending}
            onClick={() => setConfirm(true)}
          >
            {update.isPending ? "Requesting…" : updating ? "Updating…" : "Update now"}
          </button>
        </div>
        {error && !isSudoRequired(error) && !updating && (
          <p className="text-error text-sm">{updateErrorMessage(error)}</p>
        )}
        <SudoPrompt
          error={error}
          onSuccess={() => (update.error ? update.mutate() : void status.refetch())}
        />
      </div>
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Update the server now?"
        confirmLabel="Update now"
        onConfirm={() => update.mutate()}
      >
        {data?.latest && data.update_available
          ? `This installs ${data.latest.version} and restarts the server. `
          : "This reinstalls the newest build of the current channel and restarts the server. "}
        Everyone loses their connection for a minute or two; the database is kept.
      </ConfirmDialog>
    </section>
  )
}

function VersionLine({ status }: { status: SoftwareUpdateStatus }) {
  if (!status.version) {
    return <p className="text-sm">Running a development build.</p>
  }
  return (
    <p className="text-sm">
      Running <strong>{status.version}</strong>
      {status.channel === "release" && " on the release channel."}
      {status.channel === "nightly" && " on the nightly channel, which follows every push to main."}
      {status.channel === "preview" &&
        " on the preview channel, a pre-release build of a branch that is not merged yet."}
      {status.channel === null && "."}
    </p>
  )
}

function LatestLine({ status }: { status: SoftwareUpdateStatus }) {
  if (status.check_error) {
    return (
      <p className="text-base-content/70 text-sm">
        Could not check for updates: {status.check_error}
      </p>
    )
  }
  if (!status.latest) return null
  if (status.update_available) {
    return (
      <p className="text-sm">
        <strong>{status.latest.version}</strong> is available.{" "}
        <a className="link" href={status.latest.url} target="_blank" rel="noreferrer">
          What changed
        </a>
      </p>
    )
  }
  return (
    <p className="text-base-content/70 text-sm">
      Up to date: {status.latest.version} is the newest build on this channel.
    </p>
  )
}

function methodDescription(method: SoftwareUpdateStatus["method"] | undefined) {
  switch (method) {
    case "systemd":
      return "Updating downloads the newest build of this channel from GitHub, installs it next to the previous release, and restarts the service."
    case "watchtower":
      return "Updating asks Watchtower to pull the newest image and recreate the container with the same settings."
    default:
      return "Updates cannot be started from here. In the Proxmox LXC, run `update` once so the updater installs its systemd hook; with Docker Compose, enable the self-update profile described in the README."
  }
}

function describeVersion(version: string | null) {
  return version ?? "a development build"
}

function requestedAt(status: SoftwareUpdateStatus | undefined) {
  if (!status?.requested_at) return ""
  return ` at ${new Date(status.requested_at).toLocaleTimeString([], { timeStyle: "short" })}`
}

function updateErrorMessage(error: unknown) {
  if (error instanceof ApiError && error.status === 409) return "An update is already running."
  if (error instanceof ApiError && error.status === 502) {
    return "The updater did not respond. Check the server logs."
  }
  return errorMessage(error)
}
