import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { BookOpen } from "lucide-react"
import { SudoPrompt } from "@/components/sudo-prompt"
import {
  formatDate,
  getRulesStatus,
  rulingStatusKey,
  syncRules,
  type RulesStatus,
} from "@/features/rulings/rulings"
import { errorMessage, isSudoRequired } from "@/lib/auth"

const rulesKey = ["admin", "rules"] as const

function summary(status: RulesStatus) {
  if (status.rule_count === 0) return "The Comprehensive Rules have not been downloaded yet."
  const effective = status.effective_on ? ` effective ${formatDate(status.effective_on)}` : ""
  return `Installed: the Comprehensive Rules${effective}, ${status.rule_count.toLocaleString()} rules and ${status.glossary_count.toLocaleString()} glossary terms.`
}

/** The Comprehensive Rules AI rulings look rules up in, kept current from Wizards' rules page. */
export function ComprehensiveRules() {
  const queryClient = useQueryClient()
  const status = useQuery({ queryKey: rulesKey, queryFn: getRulesStatus })
  const sync = useMutation({
    mutationFn: syncRules,
    onSuccess: (result) => {
      queryClient.setQueryData(rulesKey, result.status)
      void queryClient.invalidateQueries({ queryKey: rulingStatusKey })
    },
    onError: () => void queryClient.invalidateQueries({ queryKey: rulesKey }),
  })
  const data = status.data

  return (
    <section
      className="card bg-base-200 border-base-300 border"
      aria-labelledby="comprehensive-rules-heading"
    >
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2
          id="comprehensive-rules-heading"
          className="flex items-center gap-2 text-lg font-semibold"
        >
          <BookOpen className="size-5" /> Comprehensive Rules
        </h2>
        <p className="text-base-content/70 text-sm">
          AI rulings look rules up in the Magic Comprehensive Rules. The server checks
          magic.wizards.com/rules daily and installs each new release; check by hand right after a
          release to pick it up sooner.
        </p>
        <div>
          <button
            type="button"
            className="btn btn-primary"
            disabled={sync.isPending}
            onClick={() => sync.mutate()}
          >
            {sync.isPending ? "Checking…" : "Check for new rules"}
          </button>
        </div>
        {data && (
          <p className="text-sm" role="status">
            {summary(data)}
            {sync.data &&
              (sync.data.outcome === "installed"
                ? " Just installed."
                : " This is the newest release.")}
          </p>
        )}
        {data?.checked_at && (
          <p className="text-base-content/60 text-xs">
            Last checked {new Date(data.checked_at).toLocaleString()}
            {data.source_url && (
              <>
                {" · "}
                <a href={data.source_url} className="link" target="_blank" rel="noreferrer">
                  source
                </a>
              </>
            )}
          </p>
        )}
        {data?.last_error && !sync.error && (
          <p className="text-error text-sm break-words">Last check failed: {data.last_error}</p>
        )}
        {status.error && <p className="text-error text-sm">{errorMessage(status.error)}</p>}
        {sync.error && !isSudoRequired(sync.error) && (
          <p className="text-error text-sm break-words">{errorMessage(sync.error)}</p>
        )}
        <SudoPrompt error={sync.error} onSuccess={() => sync.mutate()} />
      </div>
    </section>
  )
}
