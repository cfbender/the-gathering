import { Link } from "@tanstack/react-router"
import { Skull } from "lucide-react"
import type { KillStats as KillStatsData } from "@/lib/stats"

export function KillStats({ stats }: { stats: KillStatsData }) {
  return (
    <section
      aria-label="Kills"
      className="border-base-300 bg-base-200/60 min-w-0 rounded-xl border p-5"
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-primary text-xs font-bold uppercase">Taking players out</p>
          <h2 className="text-xl font-bold">Kills</h2>
        </div>
        <Skull className="text-accent size-5" />
      </div>
      <p className="mt-4 flex items-baseline gap-2">
        <strong className="text-3xl font-black tabular-nums">
          {stats.recorded_seats ? stats.total : "—"}
        </strong>
        <span className="text-base-content/60 text-sm">recorded kills</span>
      </p>
      <p className="text-base-content/60 mt-2 text-sm">
        Counts recorded for {stats.recorded_seats} of {stats.total_seats} player appearances.
        Averages exclude missing counts, not zeros.
      </p>
      {stats.players.length ? (
        <div className="mt-5">
          <div className="text-base-content/50 grid grid-cols-[1fr_3rem_4.5rem] gap-2 text-xs">
            <span>Player · recorded games</span>
            <span className="text-right">Kills</span>
            <span className="text-right">Per game</span>
          </div>
          <ul className="divide-base-300 mt-1 divide-y">
            {stats.players.map((player) => (
              <li key={player.id}>
                <Link
                  to="/players/$playerId"
                  params={{ playerId: String(player.id) }}
                  className="hover:bg-base-300/40 grid grid-cols-[1fr_3rem_4.5rem] items-baseline gap-2 rounded py-2.5 text-sm"
                >
                  <span className="min-w-0 break-words font-medium">
                    {player.name}{" "}
                    <span className="text-base-content/50 font-normal">
                      · {player.recorded_games}
                    </span>
                  </span>
                  <strong className="text-primary text-right tabular-nums">{player.kills}</strong>
                  <span className="text-base-content/70 text-right tabular-nums">
                    {player.average.toFixed(2)}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <p className="text-base-content/55 mt-5 text-sm">No kill counts in this range yet.</p>
      )}
    </section>
  )
}
