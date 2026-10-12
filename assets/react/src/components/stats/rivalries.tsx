import { GameChangerBadge } from "@/components/game-changer-badge"
import { Link } from "@tanstack/react-router"
import { CardArtBackground } from "@/components/card-art-background"
import { CardHover } from "@/components/card-hover"
import { ColorIdentity } from "@/components/mana-symbols"
import { PlayerAvatar } from "@/components/player-avatar"
import {
  favoritePrey,
  favoriteVictim,
  killRival,
  nemesisCommander,
  nemesisPlayer,
} from "@/lib/rivals"
import type { CommanderStats, HeadToHead, RivalCommander } from "@/lib/stats"
import { gamesLink, type GamesLinkScope } from "@/features/games/game-filters"
import type { PlayerSummary } from "@/features/games/games"
import { useCurrentUser } from "@/lib/auth"

type RivalryLink =
  | { to: "/players/$playerId"; params: { playerId: string } }
  | { to: "/commanders/$commanderId"; params: { commanderId: string } }
  | ReturnType<typeof gamesLink>

export function RivalryCallout({
  label,
  name,
  count,
  detail,
  link,
  artCropUrl,
  avatarUrl,
  colorIdentity,
}: {
  label: string
  name: string
  count: number
  detail: string
  link: RivalryLink
  artCropUrl?: string | null
  avatarUrl?: string | null
  colorIdentity?: string | null
}) {
  const linkClassName =
    "absolute inset-0 z-20 rounded-xl focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
  const accessibleName = `${name}: ${count} ${detail}`
  return (
    <div className="border-base-300 bg-base-200 group hover:border-primary relative min-w-0 overflow-hidden rounded-xl border p-4 transition-all hover:-translate-y-0.5 hover:shadow-lg">
      <CardArtBackground imageUrl={artCropUrl} interactive />
      <div className="relative z-10 flex flex-col gap-3">
        <div className="flex items-start justify-between gap-2">
          <p className="text-primary text-xs font-bold tracking-wide uppercase">{label}</p>
          {colorIdentity && <ColorIdentity colors={colorIdentity} />}
        </div>
        <div className="border-base-300/70 bg-base-100/75 rounded-box flex items-center gap-3 border px-3 py-2 shadow-sm backdrop-blur">
          {avatarUrl !== undefined && (
            <PlayerAvatar name={name} avatarUrl={avatarUrl} size="sm" className="shrink-0" />
          )}
          <div className="min-w-0">
            <strong className="text-base-content block truncate" title={name}>
              {name}
            </strong>
            <span className="text-base-content/70 text-sm tabular-nums">
              {count} {detail}
            </span>
          </div>
        </div>
      </div>
      {link.to === "/games" ? (
        <Link {...link} className={linkClassName} aria-label={accessibleName} />
      ) : link.to === "/players/$playerId" ? (
        <Link
          to="/players/$playerId"
          params={link.params}
          className={linkClassName}
          aria-label={accessibleName}
        />
      ) : (
        <Link
          to="/commanders/$commanderId"
          params={link.params}
          className={linkClassName}
          aria-label={accessibleName}
        />
      )}
    </div>
  )
}

export function Rivalries({
  player,
  headToHead,
  commanders,
  games,
}: {
  player: Pick<PlayerSummary, "name" | "user_id">
  headToHead: HeadToHead[]
  commanders: RivalCommander[]
  games: GamesLinkScope & { player_id: number }
}) {
  const viewer = useCurrentUser()
  const isOwnProfile = viewer.data !== undefined && viewer.data.id === player.user_id
  const playerLabel = isOwnProfile ? "you" : player.name
  const nemesis = nemesisPlayer(headToHead)
  const victim = favoriteVictim(headToHead)
  const commanderNemesis = nemesisCommander(commanders)
  const commanderPrey = favoritePrey(commanders)
  const mostKilled = killRival(headToHead, "kills")
  const killedByMost = killRival(headToHead, "killed_by")
  const callouts = [
    {
      label: "Nemesis",
      row: nemesis,
      value: nemesis?.losses,
      result: ["loss", "losses"],
      kind: "player",
      filters: { player_id: games.player_id, winner_id: nemesis?.id },
    },
    {
      label: "Favorite victim",
      row: victim,
      value: victim?.wins,
      result: ["win", "wins"],
      kind: "player",
      filters: { player_id: games.player_id, opponent_id: victim?.id, player_result: "win" },
    },
    {
      label: "Nemesis commander",
      row: commanderNemesis,
      value: commanderNemesis?.beat_me,
      result: ["loss", "losses"],
      kind: "commander",
      filters: {
        player_id: games.player_id,
        opponent_commander: commanderNemesis?.name,
        opponent_result: "win",
      },
    },
    {
      label: "Favorite prey",
      row: commanderPrey,
      value: commanderPrey?.beaten,
      result: ["win", "wins"],
      kind: "commander",
      filters: {
        player_id: games.player_id,
        opponent_commander: commanderPrey?.name,
        player_result: "win",
      },
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
        <p className="text-base-content/60 text-sm">Select a rivalry or count to see its games.</p>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {callouts.map(({ label, row, value, result, kind, filters }) => {
          if (!row || value === undefined) return null
          const commander = kind === "commander" ? row : null
          return (
            <RivalryCallout
              key={label}
              label={label}
              name={row.name}
              count={value}
              detail={value === 1 ? result[0] : result[1]}
              artCropUrl={commander?.art_crop_url}
              avatarUrl={kind === "player" ? row.avatar_url : undefined}
              colorIdentity={commander?.color_identity}
              link={gamesLink(games, filters)}
            />
          )
        })}
      </div>
      <section className="space-y-3" aria-label="Kill rivalries">
        <div>
          <h4 className="font-bold">Kill rivalries</h4>
          <p className="text-base-content/60 text-sm">
            Named victims only, in the selected date range. Number-only kills are not attributed.
          </p>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          {[
            {
              label: "Most killed",
              row: mostKilled,
              count: mostKilled?.kills,
              detail: `killed by ${playerLabel}`,
              killer: games.player_id,
              victim: mostKilled?.id,
            },
            {
              label: "Killed by most",
              row: killedByMost,
              count: killedByMost?.killed_by,
              detail: `killed ${playerLabel}`,
              killer: killedByMost?.id,
              victim: games.player_id,
            },
          ].map(({ label, row, count, detail, killer, victim }) =>
            row && count !== undefined ? (
              <RivalryCallout
                key={label}
                label={label}
                name={row.name}
                count={count}
                detail={`${count === 1 ? "time" : "times"} ${detail}`}
                avatarUrl={row.avatar_url}
                link={gamesLink(games, { killer_id: killer, victim_id: victim })}
              />
            ) : (
              <div key={label} className="border-base-300 rounded-xl border border-dashed p-4">
                <p className="text-primary text-xs font-bold tracking-wide uppercase">{label}</p>
                <p className="text-base-content/60 mt-3 text-sm">
                  No named {label === "Most killed" ? "victims" : "killers"} in this date range.
                </p>
              </div>
            ),
          )}
        </div>
      </section>
      {commanders.length > 0 && (
        <div className="border-base-300 bg-base-200/60 overflow-hidden rounded-xl border">
          <div className="grid grid-cols-[minmax(0,1fr)_repeat(3,3.25rem)] gap-2 border-b border-base-300 px-4 py-2 text-right text-xs font-bold uppercase text-base-content/50 sm:grid-cols-[minmax(0,1fr)_repeat(3,4rem)]">
            <span className="text-left">Rival commander</span>
            <span>Faced</span>
            <span>{isOwnProfile ? "Beat me" : "Wins"}</span>
            <span>Beaten</span>
          </div>
          {commanders.slice(0, 6).map((commander) => (
            <div
              key={commander.id}
              className="grid grid-cols-[minmax(0,1fr)_repeat(3,3.25rem)] items-center gap-2 border-b border-base-300/70 px-4 py-2.5 text-right text-sm last:border-b-0 hover:bg-base-300/40 sm:grid-cols-[minmax(0,1fr)_repeat(3,4rem)]"
            >
              <span className="flex min-w-0 items-center gap-2 text-left font-medium">
                <ColorIdentity
                  colors={commander.color_identity ?? ""}
                  className="hidden sm:inline-flex"
                />
                <CardHover
                  id={null}
                  name={commander.name}
                  gameChanger={commander.game_changer}
                  imageUrl={commander.image_url}
                  artCropUrl={commander.art_crop_url}
                >
                  <span className="truncate">{commander.name}</span>
                </CardHover>
                <GameChangerBadge gameChanger={commander.game_changer} compact />
              </span>
              {[
                { count: commander.faced, label: "faced", filters: {} },
                {
                  count: commander.beat_me,
                  label: `beat ${playerLabel}`,
                  filters: { opponent_result: "win" },
                },
                {
                  count: commander.beaten,
                  label: `beaten by ${playerLabel}`,
                  filters: { player_result: "win" },
                },
              ].map(({ count, label, filters }) => (
                <Link
                  key={label}
                  {...gamesLink(games, {
                    player_id: games.player_id,
                    opponent_commander: commander.name,
                    ...filters,
                  })}
                  aria-label={`${commander.name}: ${count} ${label}. Show games`}
                  className="hover:text-primary rounded py-1 underline decoration-dotted underline-offset-4"
                >
                  {count}
                </Link>
              ))}
            </div>
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
          name={nemesis.name}
          count={nemesis.wins}
          detail={nemesis.wins === 1 ? "win against this commander" : "wins against this commander"}
          avatarUrl={nemesis.avatar_url}
          link={{ to: "/players/$playerId", params: { playerId: String(nemesis.id) } }}
        />
      )}
      {prey && (
        <RivalryCallout
          label="Favorite victim"
          name={prey.name}
          count={prey.beaten}
          detail={prey.beaten === 1 ? "loss to this commander" : "losses to this commander"}
          avatarUrl={prey.avatar_url}
          link={{ to: "/players/$playerId", params: { playerId: String(prey.id) } }}
        />
      )}
    </section>
  )
}
