import { createFileRoute, redirect } from "@tanstack/react-router"

export const Route = createFileRoute("/table/new")({
  beforeLoad: () => {
    throw redirect({ to: "/table/$roomId", params: { roomId: crypto.randomUUID() }, replace: true })
  },
  component: () => null,
})
