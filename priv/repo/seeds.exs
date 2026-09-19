# Script for populating the database. You can run it as:
#
#     mix run priv/repo/seeds.exs
#
# Inside the script, you can read and write to any of your
# repositories directly:
#
#     TheGathering.Repo.insert!(%TheGathering.SomeSchema{})
#
# We recommend using the bang functions (`insert!`, `update!`
# and so on) as they will fail if something goes wrong.

if Mix.env() == :dev do
  alias TheGathering.Games

  player_names = ["Cody", "Mara", "Theo", "Jules", "Ren"]

  players =
    Map.new(player_names, fn name ->
      {:ok, player} = Games.find_or_create_player_by_name(name)
      {name, player}
    end)

  deck_specs = [
    {"Cody", "Birds of a Feather", "Kangee, Sky Warden", "WU"},
    {"Cody", "Grave Intentions", "Muldrotha, the Gravetide", "UBG"},
    {"Mara", "Goblin Mode", "Krenko, Mob Boss", "R"},
    {"Mara", "Court Intrigue", "Queen Marchesa", "WBR"},
    {"Theo", "Elf Service", "Lathril, Blade of the Elves", "BG"},
    {"Theo", "Deep Thoughts", "Aesi, Tyrant of Gyre Strait", "UG"},
    {"Jules", "Cat Pact", "Arahbo, Roar of the World", "WG"},
    {"Jules", "Artifact Hours", "Urza, Lord High Artificer", "U"},
    {"Ren", "Dragon Weather", "Miirym, Sentinel Wyrm", "URG"},
    {"Ren", "Everybody Hurts", "Kambal, Consul of Allocation", "WB"}
  ]

  decks =
    Map.new(deck_specs, fn {owner, name, commander, colors} ->
      {:ok, deck} =
        Games.find_or_create_deck(players[owner], name, %{
          commander_name: commander,
          color_identity: colors
        })

      {{owner, name}, deck}
    end)

  decks_by_player =
    Map.new(player_names, fn name ->
      owned = for {{^name, _deck_name}, deck} <- decks, do: deck
      {name, Enum.sort_by(owned, & &1.name)}
    end)

  winners = ["Cody", "Mara", "Cody", "Theo", "Jules", "Cody", "Ren", "Mara"]
  base = ~U[2025-11-01 19:00:00Z]

  Enum.each(0..39, fn index ->
    absent = Enum.at(player_names, rem(index, length(player_names)))
    table = Enum.reject(player_names, &(&1 == absent))
    rotated = Enum.drop(table, rem(index * 3, 4)) ++ Enum.take(table, rem(index * 3, 4))
    draw? = index in [11, 29]
    winner = Enum.at(winners, rem(index, length(winners)))
    winner = if winner in table, do: winner, else: Enum.at(table, rem(index, 4))

    seats =
      rotated
      |> Enum.with_index(1)
      |> Enum.map(fn {name, seat} ->
        deck = Enum.at(decks_by_player[name], rem(div(index, 2) + seat, 2))

        %{
          player_id: players[name].id,
          deck_id: deck.id,
          seat: seat,
          result: if(draw?, do: "draw", else: if(name == winner, do: "win", else: "loss")),
          mvp_card_name:
            if(name == winner and not draw?,
              do:
                Enum.at(
                  ["Sol Ring", "Rhystic Study", "Swords to Plowshares", "Heroic Intervention"],
                  rem(index, 4)
                )
            )
        }
      end)

    {:ok, _game} =
      Games.create_game(%{
        played_at: DateTime.add(base, index * 7, :day),
        duration_minutes: 52 + rem(index * 17, 71),
        turns: 7 + rem(index * 5, 9),
        source: "csv",
        external_id: "demo-stats-#{index + 1}",
        seats: seats
      })
  end)

  IO.puts("Seeded #{length(player_names)} players, #{map_size(decks)} decks, and 40 demo games.")
else
  IO.puts("Skipping development demo data outside MIX_ENV=dev.")
end
