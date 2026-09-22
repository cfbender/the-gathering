defmodule TheGathering.Discord.WonCommandTest do
  use TheGathering.DataCase, async: false

  alias Nostrum.Struct.Interaction
  alias TheGathering.Catalog.Card
  alias TheGathering.Discord
  alias TheGathering.Discord.{GameReport, ResultDraft, WonCommand}
  alias TheGathering.Games
  alias TheGathering.Games.Game

  setup do
    config = Application.get_env(:the_gathering, Discord, [])
    Application.put_env(:the_gathering, Discord, guild_id: "333")
    on_exit(fn -> Application.put_env(:the_gathering, Discord, config) end)
    :ok
  end

  test "six-player decoded modal flow saves only on confirmation with correctly attributed data" do
    stage(6)
    card("sol-ring", "Sol Ring")
    id = open()
    assert Repo.aggregate(Game, :count) == 0
    assert Repo.get!(ResultDraft, id).data["duration"] in ["90", "91"]

    review =
      submit(id, "details", %{
        "turns" => "7",
        "duration" => "95",
        "mvp" => "sol ring",
        "notes" => "Close finish\n@everyone"
      })

    assert review.type == 4
    assert review.data.flags == 64
    assert review.data.allowed_mentions == %{parse: []}
    assert length(review.data.components) <= 5
    click(id, "winner", "112")
    click(id, "condition", "poison")

    assert %{type: 9, data: %{components: first}} = click(id, "kills0")
    assert length(first) == 5
    assert %{type: 9, data: %{components: second}} = click(id, "kills1")
    assert [%{components: [%{custom_id: "kills_116"}]}] = second

    submit(id, "kills0", %{
      "kills_111" => "0",
      "kills_112" => "3",
      "kills_113" => "1",
      "kills_114" => "",
      "kills_115" => "0"
    })

    assert click(id, "save").data.content =~ "every kills page"
    assert Repo.aggregate(Game, :count) == 0
    submit(id, "kills1", %{"kills_116" => "1"})
    assert Repo.aggregate(Game, :count) == 0
    assert click(id, "save").data.content =~ "Recorded SB12345"

    game = Repo.one!(Game) |> then(&Games.get_game!(&1.id))
    assert game.turns == 7
    assert game.duration_minutes == 95
    assert game.win_condition == "poison"
    assert game.notes == "Close finish\n@everyone"
    seats = Map.new(game.seats, &{&1.player.discord_id, &1})
    assert seats["112"].result == "win"
    assert seats["112"].mvp_card_id == "sol-ring"
    assert seats["112"].mvp_card_name == "Sol Ring"

    assert Enum.all?(Map.delete(seats, "112"), fn {_, s} ->
             s.result == "loss" and is_nil(s.mvp_card_id)
           end)

    assert Map.new(seats, fn {id, s} -> {id, s.kills} end) == %{
             "111" => 0,
             "112" => 3,
             "113" => 1,
             "114" => nil,
             "115" => 0,
             "116" => 1
           }

    assert Discord.get_pending_by_external_id("spellbot:SB12345") == nil
    assert Repo.aggregate(ResultDraft, :count) == 0
    assert click(id, "save").data.content =~ "expired"
    assert Repo.aggregate(Game, :count) == 1
  end

  test "cancelling or dismissing a modal leaves the game pending" do
    stage(3)
    id = open()
    assert Repo.aggregate(Game, :count) == 0
    assert click(id, "cancel").data.content =~ "cancelled"
    assert Repo.get(ResultDraft, id) == nil
    assert Discord.get_pending_by_external_id("spellbot:SB12345")
    assert Repo.aggregate(Game, :count) == 0
  end

  test "ambiguous MVP is selected privately and persisted on the winner, not the reporter" do
    stage(2)
    card("ring", "Sol Ring")
    card("talisman", "Sol Talisman")
    id = open()

    review =
      submit(id, "details", %{"turns" => "", "duration" => "", "mvp" => "Sol", "notes" => ""})

    assert review.data.content =~ "Choose an MVP"
    assert length(review.data.components) == 5
    assert click(id, "mvp", "forged-id").data.content =~ "matching MVP"
    assert click(id, "mvp", "talisman").data.content =~ "MVP: Sol Talisman"
    click(id, "winner", "112")
    submit(id, "kills0", %{"kills_111" => "0", "kills_112" => "1"})
    assert click(id, "save").data.content =~ "Recorded"
    game = Repo.one!(Game) |> then(&Games.get_game!(&1.id))
    assert game.turns == nil
    assert game.duration_minutes == nil
    winner = Enum.find(game.seats, &(&1.result == "win"))
    assert winner.player.discord_id == "112"
    assert winner.mvp_card_name == "Sol Talisman"
  end

  test "invalid numbers, unrecognized MVP, and forged choices never consume the pending game" do
    stage(2)
    id = open()

    submit(id, "details", %{
      "turns" => "-1",
      "duration" => "90",
      "mvp" => "no such card",
      "notes" => "Keep this note"
    })

    submit(id, "kills0", %{"kills_111" => "0", "kills_112" => "1"})
    assert click(id, "save").data.content =~ "Turns must be"
    assert click(id, "winner", "999").data.content =~ "this game's players"
    assert click(id, "condition", "made_up").data.content =~ "valid win condition"
    assert click(id, "condition", "draw").data.content =~ "valid win condition"

    submit(id, "details", %{
      "turns" => "8",
      "duration" => "90",
      "mvp" => "no such card",
      "notes" => "Keep this note"
    })

    assert click(id, "save").data.content =~ "MVP card not found"

    submit(id, "details", %{
      "turns" => "8",
      "duration" => "90",
      "mvp" => "",
      "notes" => "Keep this note"
    })

    submit(id, "kills0", %{"kills_111" => "6", "kills_112" => "1"})
    assert click(id, "save").data.content =~ "kills must be"
    assert Repo.aggregate(Game, :count) == 0
    assert Repo.get!(ResultDraft, id).data["notes"] == "Keep this note"
    submit(id, "kills0", %{"kills_111" => "0", "kills_112" => "1"})
    assert click(id, "save").data.content =~ "Recorded"
  end

  test "draft is bound to the reporter, guild and channel, and cannot be used after expiry or roster changes" do
    report = stage(3)
    id = open()

    assert interaction(2, %{name: "won"}, "999")
           |> WonCommand.handle()
           |> get_in([:data, :content]) =~ "participant"

    assert click(id, "save", nil, "112").data.content =~ "not yours"

    for changes <- [%{guild_id: nil}, %{guild_id: 999}, %{channel_id: 999}] do
      event = interaction(3, %{custom_id: "won:#{id}:save"}) |> Map.merge(changes)
      assert WonCommand.handle(event).data.content =~ "not yours"
    end

    draft = Repo.get!(ResultDraft, id)

    draft
    |> Ecto.Changeset.change(
      expires_at: DateTime.add(DateTime.utc_now() |> DateTime.truncate(:second), -1)
    )
    |> Repo.update!()

    assert click(id, "save").data.content =~ "expired"
    second = open()
    Discord.stage_report(%{report | players: Enum.reverse(report.players)})
    assert click(second, "save").data.content =~ "changed"
    assert Repo.aggregate(Game, :count) == 0
  end

  test "a second participant's old draft and restaged completed games cannot overwrite the result" do
    report = stage(2)
    first = open()
    second = open("112")

    submit(first, "details", %{
      "turns" => "5",
      "duration" => "60",
      "mvp" => "",
      "notes" => "Original"
    })

    submit(first, "kills0", %{"kills_111" => "1", "kills_112" => "0"})
    click(first, "save")
    assert click(second, "save", nil, "112").data.content =~ "expired"
    Discord.stage_report(report)
    event = interaction(2, %{name: "won", options: [%{name: "game", type: 3, value: "SB12345"}]})
    assert WonCommand.handle(event).data.content =~ "already been recorded"
    assert Repo.one!(Game).notes == "Original"
  end

  test "editing through a button updates the private review and preserves the full note" do
    stage(2)
    id = open()
    notes = String.duplicate("A", 4000)

    review =
      submit(id, "details", %{"turns" => "8", "duration" => "91", "mvp" => "", "notes" => notes})

    assert String.length(review.data.content) < 2000
    assert Repo.get!(ResultDraft, id).data["notes"] == notes
    assert %{type: 9, data: %{components: rows}} = click(id, "details")
    assert Enum.at(rows, 3).components |> hd() |> Map.fetch!(:value) == notes

    review = submit(id, "kills0", %{"kills_111" => "1", "kills_112" => "0"}, true)
    assert review.type == 7
    refute Map.has_key?(review.data, :flags)
    assert review.data.allowed_mentions == %{parse: []}
    assert review.data.content =~ "Player 1: 1 · Player 2: 0"

    refute Enum.any?(List.flatten(Enum.map(review.data.components, & &1.components)), fn c ->
             c.custom_id == "won:#{id}:kills1"
           end)

    assert click(id, "save").data.content =~ "Recorded"
    assert Repo.one!(Game).notes == notes
  end

  test "Nostrum HTTP 204 acknowledgement is accepted for opening the modal" do
    stage(2)
    assert :ok = WonCommand.respond(interaction(2, %{name: "won"}), __MODULE__)
    assert_receive {:response, %{type: 9, data: %{components: rows}}}
    assert length(rows) == 4
    assert Enum.all?(rows, &match?(%{type: 1, components: [%{type: 4}]}, &1))
  end

  def create_response(_interaction, response) do
    send(self(), {:response, response})
    {:ok}
  end

  defp open(user \\ "111") do
    assert %{type: 9, data: %{custom_id: "won:" <> rest}} =
             WonCommand.handle(interaction(2, %{name: "won"}, user))

    [id, "details"] = String.split(rest, ":")
    id
  end

  defp click(id, action, value \\ nil, user \\ "111") do
    interaction(
      3,
      %{custom_id: "won:#{id}:#{action}", values: if(value, do: [value], else: [])},
      user
    )
    |> WonCommand.handle()
  end

  defp submit(id, action, fields, from_message \\ false) do
    components =
      Enum.map(fields, fn {key, value} ->
        %{type: 1, components: [%{type: 4, custom_id: key, value: value}]}
      end)

    extra = if from_message, do: %{message: %{id: "999", channel_id: "444"}}, else: %{}

    interaction(5, %{custom_id: "won:#{id}:#{action}", components: components}, "111", extra)
    |> WonCommand.handle()
  end

  defp interaction(type, data, user \\ "111", extra \\ %{}) do
    %{
      id: "777",
      application_id: "888",
      token: "test-only-token",
      type: type,
      guild_id: "333",
      channel_id: "444",
      member: %{user: %{id: user}},
      data: data
    }
    |> Map.merge(extra)
    |> Interaction.to_struct()
  end

  defp stage(count) do
    report = %GameReport{
      external_id: "spellbot:SB12345",
      source: "discord",
      guild_id: "333",
      channel_id: "444",
      played_at: DateTime.add(DateTime.utc_now() |> DateTime.truncate(:second), -90, :minute),
      players:
        Enum.map(
          1..count,
          &%{discord_id: to_string(110 + &1), display_name: "Player #{&1}", commander_name: nil}
        ),
      winner_discord_ids: [],
      raw: %{}
    }

    {:ok, _} = Discord.stage_report(report)
    report
  end

  defp card(id, name) do
    Repo.insert!(%Card{
      id: id,
      oracle_id: id,
      name: name,
      normalized_name: String.downcase(name),
      type_line: "Artifact",
      set_code: "tst",
      collector_number: "1",
      layout: "normal",
      rarity: "common",
      image_uris: %{},
      color_identity: [],
      colors: [],
      can_be_commander: false
    })
  end
end
