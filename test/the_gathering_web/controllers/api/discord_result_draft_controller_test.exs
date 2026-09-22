defmodule TheGatheringWeb.API.DiscordResultDraftControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias Nostrum.Struct.Interaction
  alias TheGathering.AccountsFixtures
  alias TheGathering.Discord
  alias TheGathering.Discord.{GameReport, LogCommand, PendingGame, ResultDraft, WebGameDraft}
  alias TheGathering.Games
  alias TheGathering.Games.{Deck, Game, Player}
  alias TheGathering.Repo

  setup %{conn: conn} do
    config = Application.get_env(:the_gathering, Discord, [])
    Application.put_env(:the_gathering, Discord, guild_id: "333")
    on_exit(fn -> Application.put_env(:the_gathering, Discord, config) end)

    user =
      AccountsFixtures.user_fixture()
      |> Ecto.Changeset.change(discord_id: "999")
      |> Repo.update!()

    {:ok, pending} = Discord.stage_report(report())
    %{conn: log_in_user(conn, user), user: user, pending: pending}
  end

  test "a non-player can get a private link with an optional winner mention; preview is read-only",
       %{conn: conn} do
    interaction =
      Interaction.to_struct(%{
        id: "777",
        application_id: "888",
        token: "test-only",
        type: 2,
        guild_id: "333",
        channel_id: "444",
        member: %{user: %{id: "999"}},
        data: %{name: "log", options: [%{name: "winner", type: 6, value: "222"}]}
      })

    response = LogCommand.handle(interaction)
    assert response.type == 4
    assert response.data.flags == 64
    assert response.data.allowed_mentions == %{parse: []}
    assert [%{components: [%{style: 5, url: url}]}] = response.data.components
    id = URI.decode_query(URI.parse(url).query)["discord"]
    assert URI.parse(url).path == "/games/new"
    before = Repo.aggregate(Player, :count)

    data =
      get(conn, "/api/discord/result-drafts/#{id}") |> json_response(200) |> Map.fetch!("data")

    assert data["winner_discord_id"] == "222"
    assert Enum.map(data["seats"], & &1["discord_id"]) == ["111", "222"]
    assert data["played_at"] == DateTime.to_iso8601(report().played_at)
    assert Repo.aggregate(Player, :count) == before
    assert Repo.aggregate(Game, :count) == 0
    assert {:error, :invalid_winner} = WebGameDraft.open("", "999", actor())

    assert {:error, :forbidden} =
             WebGameDraft.open("SB12345", nil, %{actor() | guild_id: "other"})

    assert {:error, :forbidden} = WebGameDraft.open("SB12345", nil, %{actor() | guild_id: ""})
  end

  test "saving creates the Discord identities and game atomically, ignores forged provenance, and consumes drafts",
       %{conn: conn, user: user, pending: pending} do
    {:ok, draft} = WebGameDraft.open("", nil, actor())
    {:ok, second} = WebGameDraft.open("", nil, actor())
    assert draft.data["winner"] == nil

    payload =
      payload()
      |> Map.merge(%{"source" => "manual", "external_id" => "forged", "created_by_user_id" => -1})

    result =
      post(conn, "/api/discord/result-drafts/#{draft.id}", %{"game" => payload})
      |> json_response(201)

    game = Games.get_game!(result["data"]["id"])
    assert game.source == "discord"
    assert game.external_id == "spellbot:SB12345"
    assert game.created_by_user_id == user.id
    assert game.notes == "Web log note"

    assert Enum.map(game.seats, &{&1.player.discord_id, &1.result, &1.kills}) == [
             {"222", "win", 1},
             {"111", "loss", 0}
           ]

    assert hd(game.seats).deck.commander_name == "Bello, Bard of the Brambles"
    assert Repo.get(PendingGame, pending.id) == nil
    assert Repo.get(ResultDraft, draft.id) == nil

    assert json_response(
             post(conn, "/api/discord/result-drafts/#{second.id}", %{"game" => payload}),
             404
           )

    assert Repo.aggregate(Game, :count) == 1
  end

  test "invalid game data rolls back new players and decks and preserves pending state", %{
    conn: conn,
    pending: pending
  } do
    {:ok, draft} = WebGameDraft.open("", nil, actor())
    players_before = Repo.aggregate(Player, :count)
    decks_before = Repo.aggregate(Deck, :count)
    invalid = Map.put(payload(), "turns", -1)

    assert json_response(
             post(conn, "/api/discord/result-drafts/#{draft.id}", %{"game" => invalid}),
             422
           )

    assert Repo.aggregate(Player, :count) == players_before
    assert Repo.aggregate(Deck, :count) == decks_before
    assert Repo.aggregate(Game, :count) == 0
    assert Repo.get(PendingGame, pending.id)
    assert Repo.get(ResultDraft, draft.id)

    assert json_response(
             post(conn, "/api/discord/result-drafts/#{draft.id}", %{"game" => payload()}),
             201
           )
  end

  test "rejects changed, duplicate or missing roster identities and another player's deck", %{
    conn: conn
  } do
    {:ok, draft} = WebGameDraft.open("", nil, actor())
    [first, last] = payload()["seats"]

    for seats <- [
          [first],
          [first, first],
          [Map.put(first, "discord_id", "999"), last],
          [nil, last]
        ] do
      attrs = Map.put(payload(), "seats", seats)

      assert json_response(
               post(conn, "/api/discord/result-drafts/#{draft.id}", %{"game" => attrs}),
               400
             )
    end

    {:ok, other} = Games.resolve_player("Other", "777")

    {:ok, deck} =
      Games.create_deck(%{player_id: other.id, name: "Other's deck", commander_name: "Bello"})

    attrs = Map.put(payload(), "seats", [Map.put(first, "deck_id", deck.id), last])

    assert json_response(
             post(conn, "/api/discord/result-drafts/#{draft.id}", %{"game" => attrs}),
             400
           )

    assert Repo.aggregate(Game, :count) == 0
  end

  test "links require authentication and owner identity, and expire or invalidate on roster change",
       %{conn: conn, user: user} do
    {:ok, draft} = WebGameDraft.open("", nil, actor())
    path = "/api/discord/result-drafts/#{draft.id}"
    assert json_response(get(build_conn(), path), 401)
    other = AccountsFixtures.user_fixture()
    other_conn = build_conn() |> log_in_user(other)
    assert json_response(get(other_conn, path), 404)
    assert json_response(post(other_conn, path, %{"game" => payload()}), 404)
    disabled = %{user | disabled_at: DateTime.utc_now()}
    assert {:error, :not_found} = WebGameDraft.preview(draft.id, disabled)
    draft |> Ecto.Changeset.change(expires_at: ~U[2020-01-01 00:00:00Z]) |> Repo.update!()
    assert json_response(get(conn, path), 404)
    {:ok, draft} = WebGameDraft.open("", nil, actor())
    Discord.stage_report(%{report() | players: Enum.reverse(report().players)})

    assert json_response(
             post(conn, "/api/discord/result-drafts/#{draft.id}", %{"game" => payload()}),
             404
           )
  end

  defp actor, do: %{discord_id: "999", guild_id: "333", channel_id: "444"}

  defp report do
    %GameReport{
      external_id: "spellbot:SB12345",
      source: "discord",
      played_at: ~U[2026-09-20 14:00:00Z],
      guild_id: "333",
      channel_id: "444",
      winner_discord_ids: [],
      raw: %{},
      players: [
        %{discord_id: "111", display_name: "Aria", commander_name: nil},
        %{discord_id: "222", display_name: "Bryn", commander_name: nil}
      ]
    }
  end

  defp payload do
    %{
      "played_at" => "2026-09-20T14:00:00Z",
      "turns" => 8,
      "duration_minutes" => 72,
      "win_condition" => "combat_damage",
      "notes" => "Web log note",
      "seats" => [
        %{
          "discord_id" => "222",
          "result" => "win",
          "kills" => 1,
          "deck" => %{
            "name" => "Raccoon",
            "commander_name" => "Bello, Bard of the Brambles",
            "color_identity" => "RG"
          }
        },
        %{"discord_id" => "111", "result" => "loss", "kills" => 0}
      ]
    }
  end
end
