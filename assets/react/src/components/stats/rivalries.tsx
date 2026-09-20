import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import { CardArtBackground } from "@/components/card-art-background"
import { ColorIdentity } from "@/components/mana-symbols"
import { favoritePrey, favoriteVictim, nemesisCommander, nemesisPlayer } from "@/lib/rivals"
import type { CommanderStats, HeadToHead, RivalCommander } from "@/lib/stats"

export function RivalryCallout({
  label,
  count,
  detail,
  children,
  artCropUrl,
  colorIdentity,
}: {
  label: string
  count: number
  detail: string
  children: ReactNode
  artCropUrl?: string | null
  colorIdentity?: string | null
}) {
  return (
    <div className="border-base-300 bg-base-200 group relative min-w-0 overflow-hidden rounded-xl border p-4">
      <CardArtBackground imageUrl={artCropUrl} interactive />
      <div className="relative z-10">
        <div className="mb-3 flex items-start justify-between gap-2">
          <p className="text-primary text-xs font-bold tracking-wide uppercase">{label}</p>
          {colorIdentity && <ColorIdentity colors={colorIdentity} />}
        </div>
        <strong className="block max-w-[85%] truncate">{children}</strong>
        <span className="text-base-content/70 text-sm tabular-nums">
          {count} {detail}
        </span>
      </div>
    </div>
  )
}

export function Rivalries({
  headToHead,
  commanders,
}: {
  headToHead: HeadToHead[]
  commanders: RivalCommander[]
}) {
  const nemesis = nemesisPlayer(headToHead)
  const victim = favoriteVictim(headToHead)
  const commanderNemesis = nemesisCommander(commanders)
  const commanderPrey = favoritePrey(commanders)
  const callouts = [
    {
      label: "Nemesis",
      row: nemesis,
      value: nemesis?.losses,
      result: "loss",
      kind: "player",
    },
    {
      label: "Favorite victim",
      row: victim,
      value: victim?.wins,
      result: "win",
      kind: "player",
    },
    {
      label: "Nemesis commander",
      row: commanderNemesis,
      value: commanderNemesis?.beat_me,
      result: "loss",
      kind: "commander",
    },
    {
      label: "Favorite prey",
      row: commanderPrey,
      value: commanderPrey?.beaten,
      result: "win",
      kind: "commander",
    },
  ] as const

  if (!headToHead.length && !commanders.length) return null

  return (
    <section className="space-y-4" aria-labelledby="rivalries-heading">
      <div>
        <p className="text-primary text-xs font-bold tracking-wider uppercase">Matchups</p>
        <h3 id="rivalries-heading" className="text-xl font-bold">
          Rivalries
        </h3>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {callouts.map(({ label, row, value, result, kind }) => {
          if (!row || value === undefined) return null
          const commander = kind === "commander" ? row : null
          return (
            <RivalryCallout
              key={label}
              label={label}
              count={value}
              detail={`${result}${value === 1 ? "" : "s"}`}
              artCropUrl={commander?.art_crop_url}
              colorIdentity={commander?.color_identity}
            >
              {kind === "player" ? (
                <Link
                  to="/players/$playerId"
                  params={{ playerId: String(row.id) }}
                  className="bg-base-100/75 text-base-content block truncate rounded px-1.5 py-0.5 underline decoration-base-content/40 underline-offset-2"
                >
                  {row.name}
                </Link>
              ) : (
                <Link
                  to="/commanders/$commanderId"
                  params={{ commanderId: String(row.id) }}
                  className="bg-base-100/75 text-base-content block truncate rounded px-1.5 py-0.5 underline decoration-base-content/40 underline-offset-2"
                >
                  {row.name}
                </Link>
              )}
            </RivalryCallout>
          )
        })}
      </div>
      {commanders.length > 0 && (
        <div className="border-base-300 bg-base-200/60 overflow-hidden rounded-xl border">
          <div className="grid grid-cols-[minmax(0,1fr)_repeat(3,4rem)] gap-2 border-b border-base-300 px-4 py-2 text-right text-xs font-bold uppercase text-base-content/50">
            <span className="text-left">Rival commander</span>
            <span>Faced</span>
            <span>Beat me</span>
            <span>Beaten</span>
          </div>
          {commanders.slice(0, 6).map((commander) => (
            <Link
              key={commander.id}
              to="/commanders/$commanderId"
              params={{ commanderId: commander.id }}
              className="grid grid-cols-[minmax(0,1fr)_repeat(3,4rem)] items-center gap-2 border-b border-base-300/70 px-4 py-2.5 text-right text-sm last:border-b-0 hover:bg-base-300/40"
            >
              <span className="flex min-w-0 items-center gap-2 text-left font-medium">
                <ColorIdentity colors={commander.color_identity ?? ""} />
                <span className="truncate">{commander.name}</span>
              </span>
              <span>{commander.faced}</span>
              <span>{commander.beat_me}</span>
              <span>{commander.beaten}</span>
            </Link>
          ))}
        </div>
      )}
    </section>
  )
}

export function CommanderRivalries({ opponents }: { opponents: CommanderStats["opponents"] }) {
  if (!opponents.length) return null

  const nemesis = favoriteVictim(opponents)
  const prey = favoritePrey(
    opponents.map((opponent) => ({
      ...opponent,
      faced: opponent.games,
      beat_me: 0,
    })),
  )

  return (
    <section className="grid gap-3 sm:grid-cols-2" aria-label="Commander rivalries">
      {nemesis && (
        <RivalryCallout
          label="Nemesis"
          count={nemesis.wins}
          detail={nemesis.wins === 1 ? "win against this commander" : "wins against this commander"}
        >
          <Link
            to="/players/$playerId"
            params={{ playerId: String(nemesis.id) }}
            className="text-primary underline decoration-primary/30 underline-offset-2"
          >
            {nemesis.name}
          </Link>
        </RivalryCallout>
      )}
      {prey && (
        <RivalryCallout
          label="Favorite victim"
          count={prey.beaten}
          detail={prey.beaten === 1 ? "loss to this commander" : "losses to this commander"}
        >
          <Link
            to="/players/$playerId"
            params={{ playerId: String(prey.id) }}
            className="text-primary underline decoration-primary/30 underline-offset-2"
          >
            {prey.name}
          </Link>
        </RivalryCallout>
      )}
    </section>
  )
}
