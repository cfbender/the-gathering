import { useMutation } from "@tanstack/react-query"
import { Download, Ellipsis } from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useToast } from "@/components/ui/toast"
import { api } from "@/lib/api"
import { errorMessage } from "@/lib/auth"

async function downloadExport() {
  const data = await api<Record<string, unknown>>("/api/exports/portable")
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }),
  )
  const link = document.createElement("a")
  link.href = url
  link.download = `the-gathering-${new Date().toISOString().slice(0, 10)}.json`
  link.click()
  URL.revokeObjectURL(url)
}

/** The games page's overflow menu. Exporting downloads the whole playgroup history as the
 * portable JSON file that Import → Transfer reads on another instance. */
export function GamesMenu() {
  const { toast } = useToast()
  const download = useMutation({
    mutationFn: downloadExport,
    onError: (error) =>
      toast({ message: errorMessage(error) ?? "Export failed. Try again.", tone: "error" }),
  })

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button type="button" className="btn btn-ghost btn-square" aria-label="More game actions">
          <Ellipsis className="size-5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuItem disabled={download.isPending} onSelect={() => download.mutate()}>
          <Download className="size-4" />
          {download.isPending ? "Preparing export…" : "Export history (JSON)"}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
