import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Scale } from "lucide-react"
import { useState, type FormEvent } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { Input } from "@/components/ui/input"
import {
  formatDate,
  formatUsd,
  getAiSettings,
  rulingStatusKey,
  updateAiSettings,
  type AiSettingsUpdate,
} from "@/features/rulings/rulings"
import { errorMessage, isSudoRequired } from "@/lib/auth"
import { formValue } from "@/lib/form"

const settingsKey = ["admin", "ai-settings"] as const

/** OpenRouter credentials, model, and the monthly spend limit per member for AI rulings at
 * the table, with this month's spending. */
export function AiRulingsSettings() {
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: settingsKey, queryFn: getAiSettings })
  // Remounts the form after a save so its fields show the stored values.
  const [version, setVersion] = useState(0)
  const save = useMutation({
    mutationFn: updateAiSettings,
    onSuccess: (data) => {
      queryClient.setQueryData(settingsKey, data)
      void queryClient.invalidateQueries({ queryKey: rulingStatusKey })
      setVersion((value) => value + 1)
    },
  })
  const data = settings.data

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const update: AiSettingsUpdate = { model: formValue(form, "model").trim() || null }
    const key = formValue(form, "api_key").trim()
    if (key) update.api_key = key
    const limit = formValue(form, "monthly_limit_usd").trim()
    update.monthly_limit_usd = limit === "" ? null : Number(limit)
    save.mutate(update)
  }

  const fieldError = (field: string) => {
    const message = errorMessage(save.error, field)
    return message && <p className="text-error text-xs">{message}</p>
  }

  return (
    <section
      className="card bg-base-200 border-base-300 border"
      aria-labelledby="ai-rulings-heading"
    >
      <form key={version} className="card-body gap-3 p-4 sm:p-5" onSubmit={submit}>
        <h2 id="ai-rulings-heading" className="flex items-center gap-2 text-lg font-semibold">
          <Scale className="size-5" /> AI rulings
        </h2>
        <p className="text-base-content/70 text-sm">
          Players can ask an AI judge for rulings from the Rules tab at the table. It answers with a
          model on OpenRouter that looks up the Comprehensive Rules and each card&apos;s Oracle text
          and rulings in the local catalog. Questions are billed to this OpenRouter key; saving
          checks the key and that the model supports tool calling.
        </p>
        {settings.isPending && <div className="skeleton h-24 w-full" />}
        {settings.error && !isSudoRequired(settings.error) && (
          <p className="text-error text-sm">{errorMessage(settings.error)}</p>
        )}
        {data && (
          <>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="grid gap-1 text-sm">
                <span className="font-medium">Model ID</span>
                <Input
                  name="model"
                  defaultValue={data.model ?? ""}
                  placeholder="anthropic/claude-sonnet-5.5"
                  autoComplete="off"
                  spellCheck={false}
                />
                <span className="text-base-content/60 text-xs">
                  Any OpenRouter model ID with tool calling.
                </span>
                {fieldError("model")}
              </label>
              <label className="grid gap-1 text-sm">
                <span className="font-medium">OpenRouter API key</span>
                <Input
                  name="api_key"
                  type="password"
                  placeholder={
                    data.has_api_key ? "Saved. Enter a new key to replace it" : "sk-or-…"
                  }
                  autoComplete="off"
                />
                <span className="text-base-content/60 text-xs">
                  Stored encrypted and never shown again.
                </span>
                {fieldError("api_key")}
              </label>
              <label className="grid gap-1 text-sm">
                <span className="font-medium">Monthly limit per member (USD)</span>
                <Input
                  name="monthly_limit_usd"
                  type="number"
                  min={0}
                  step={0.01}
                  defaultValue={data.monthly_limit_usd ?? ""}
                  placeholder="No limit"
                />
                <span className="text-base-content/60 text-xs">
                  Each member&apos;s spending resets on the first of the month (UTC). Administrators
                  are not limited. Leave blank for no limit.
                </span>
                {fieldError("monthly_limit_usd")}
              </label>
            </div>
            <div className="flex flex-wrap gap-2">
              <button type="submit" className="btn btn-primary" disabled={save.isPending}>
                {save.isPending ? "Checking with OpenRouter…" : "Save"}
              </button>
              {data.has_api_key && (
                <button
                  type="button"
                  className="btn btn-ghost"
                  disabled={save.isPending}
                  onClick={() => save.mutate({ api_key: null })}
                >
                  Remove key
                </button>
              )}
            </div>
            {save.error && !isSudoRequired(save.error) && errorMessage(save.error) && (
              <p className="text-error text-sm">{errorMessage(save.error)}</p>
            )}
            <SudoPrompt
              error={save.error}
              onSuccess={() => save.variables && save.mutate(save.variables)}
            />
            <div className="grid gap-1">
              <h3 className="text-sm font-semibold">
                Spending since {formatDate(data.usage_month)}
              </h3>
              {data.usage.length === 0 ? (
                <p className="text-base-content/60 text-sm">No questions this month.</p>
              ) : (
                <table className="table-sm table w-full max-w-md">
                  <thead>
                    <tr>
                      <th>Member</th>
                      <th className="text-right">Questions</th>
                      <th className="text-right">Spent</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.usage.map((member) => (
                      <tr key={member.user_id}>
                        <td>{member.display_name}</td>
                        <td className="text-right tabular-nums">{member.questions}</td>
                        <td className="text-right tabular-nums">{formatUsd(member.spent_usd)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </>
        )}
      </form>
    </section>
  )
}
