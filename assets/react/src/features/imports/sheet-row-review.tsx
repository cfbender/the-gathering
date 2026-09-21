import { Link } from "@tanstack/react-router"
import type { SheetInput, SheetPreview, SheetRow } from "./sheet-import"

interface Props {
  row: SheetRow
  preview: SheetPreview
  input: SheetInput
  dirty: boolean
  onChange: (input: SheetInput) => void
}

export function SheetRowReview({ row, preview, input, dirty, onChange }: Props) {
  const action = !dirty || row.imported_id ? row.action : (input.actions[row.key] ?? row.action)
  const target = row.candidates.find((game) => game.id === action) ?? row.target
  const draw = row.winner === "" || row.winner.toLowerCase() === "n/a"

  return (
    <article
      className="card border-base-300 bg-base-200 border"
      aria-label={`Sheet row ${row.line}`}
    >
      <div className="card-body gap-4 p-4 sm:p-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h3 className="font-bold">
            Row {row.line} · {row.date ?? "Invalid date"} · {draw ? "Draw" : row.winner}
          </h3>
          {row.imported_id ? (
            <Link className="link" to="/games/$gameId" params={{ gameId: String(row.imported_id) }}>
              Already reconciled → Game #{row.imported_id}
            </Link>
          ) : (
            <label className="flex max-w-full flex-col gap-1 text-xs">
              Action for row {row.line}
              <select
                className="select select-bordered w-full max-w-96 truncate pr-10"
                value={action}
                onChange={(event) =>
                  onChange({
                    ...input,
                    actions: {
                      ...input.actions,
                      [row.key]:
                        event.target.value === "create" || event.target.value === "skip"
                          ? event.target.value
                          : Number(event.target.value),
                    },
                  })
                }
              >
                <option value="skip">Skip — leave existing history alone</option>
                <option value="create">Create a missing game</option>
                {row.candidates.map((game) => (
                  <option key={game.id} value={game.id}>
                    Update #{game.id} · {game.played_at.slice(0, 10)} ·{" "}
                    {game.seats.find((seat) => seat.result === "win")?.player ?? "Draw"}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span>{row.match_reason}</span>
          {row.target && (
            <Link className="link" to="/games/$gameId" params={{ gameId: String(row.target.id) }}>
              Game #{row.target.id}
            </Link>
          )}
        </div>
        {dirty ? (
          <p className="text-warning text-sm">Refresh preview to recalculate changes.</p>
        ) : row.status === "unchanged" ? (
          <p className="text-base-content/70 text-sm">No stored values would change. Skipped.</p>
        ) : row.changes.length > 0 ? (
          <dl
            className="divide-base-300 divide-y text-sm"
            aria-label={`Changes for row ${row.line}`}
          >
            {row.changes.map((change, index) => (
              <div key={index} className="grid gap-2 py-2 sm:grid-cols-[9rem_1fr_1fr]">
                <dt className="font-semibold">
                  {change.player && `${change.player} · `}
                  {change.field}
                </dt>
                <dd className="min-w-0 whitespace-pre-wrap break-words">
                  <span className="text-base-content/60 mr-2 text-xs">Before</span>
                  {change.before ?? (change.field === "kills" ? "Unknown" : "None")}
                </dd>
                <dd className="min-w-0 whitespace-pre-wrap break-words">
                  <span className="text-primary mr-2 text-xs">After</span>
                  {change.after ?? "None"}
                </dd>
              </div>
            ))}
          </dl>
        ) : null}
        <details>
          <summary className="cursor-pointer text-sm font-semibold">
            Review players and deck mappings
          </summary>
          <div className="mt-4 grid min-w-0 gap-4 lg:grid-cols-2">
            <div className="min-w-0">
              <h4 className="mb-2 text-sm font-bold">From sheet</h4>
              <ul className="space-y-3">
                {row.seats.map((seat, index) => (
                  <li key={`${seat.player}-${index}`} className="text-sm">
                    <div>
                      <strong>{seat.player}</strong> · {seat.deck} · {seat.result} · Kills:{" "}
                      {seat.kills ?? "unknown"}
                    </div>
                    <label className="mt-1 flex flex-col gap-1 text-xs">
                      Deck mapping for {seat.player} ({seat.deck})
                      <select
                        className="select select-sm w-full truncate pr-10"
                        value={input.decks[seat.deck_key] ?? ""}
                        onChange={(event) => {
                          const decks = { ...input.decks }
                          if (event.target.value === "") delete decks[seat.deck_key]
                          else
                            decks[seat.deck_key] =
                              event.target.value === "new" ? "new" : Number(event.target.value)
                          onChange({ ...input, decks })
                        }}
                      >
                        <option value="">Keep existing / exact-name match</option>
                        <option value="new">Create using sheet deck / commander name</option>
                        {preview.decks
                          .filter((deck) => deck.player_id === seat.player_id)
                          .map((deck) => (
                            <option key={deck.id} value={deck.id}>
                              {deck.name} — {deck.commander_name}
                            </option>
                          ))}
                      </select>
                    </label>
                  </li>
                ))}
              </ul>
              <p className="text-base-content/70 mt-3 whitespace-pre-wrap text-sm">
                {row.notes || "No sheet notes; existing notes will be kept."}
              </p>
            </div>
            <div className="bg-base-100 min-w-0 rounded-lg p-3 text-sm">
              <h4 className="mb-2 font-bold">
                {target ? `Existing game #${target.id}` : "Existing game"}
              </h4>
              {target ? (
                <>
                  <ul className="space-y-2">
                    {target.seats.map((seat) => (
                      <li key={seat.player_id}>
                        <strong>{seat.player}</strong> · {seat.deck ?? "No deck"} · {seat.result} ·
                        Kills: {seat.kills ?? "unknown"}
                      </li>
                    ))}
                  </ul>
                  <p className="text-base-content/70 mt-3 whitespace-pre-wrap">
                    {target.notes ?? "No notes"}
                  </p>
                  <Link
                    className="link mt-3 inline-block"
                    to="/games/$gameId"
                    params={{ gameId: String(target.id) }}
                    target="_blank"
                  >
                    Open game in a new tab
                  </Link>
                </>
              ) : (
                <p className="text-base-content/60">
                  No unambiguous match. Choose a nearby game to compare or create a missing game.
                  Candidates include the previous and next day for timezone differences.
                </p>
              )}
            </div>
          </div>
        </details>
        {row.warnings.map((warning) => (
          <p key={warning} className="text-warning text-sm">
            {warning}
          </p>
        ))}
        {row.errors.length > 0 && (
          <ul
            className="text-error list-inside list-disc text-sm"
            aria-label={`Row ${row.line} issues`}
          >
            {row.errors.map((error) => (
              <li key={error}>{error}</li>
            ))}
          </ul>
        )}
      </div>
    </article>
  )
}
