defmodule TheGathering.Discord.NewGameTest do
  use TheGathering.DataCase, async: false
  alias Nostrum.Api.Helpers
  alias Nostrum.Struct.Interaction

  alias TheGathering.Discord.{
    NewGameCommand,
    NewGameMessage,
    NewGameScheduler,
    ScheduledGame,
    ScheduledGames
  }

  alias TheGathering.DiscordNewGameAPI, as: API
  @now ~U[2026-09-23 18:00:00Z]

  setup do
    original = Application.get_env(:the_gathering, TheGathering.Discord, [])

    Application.put_env(:the_gathering, TheGathering.Discord,
      guild_id: "333",
      default_timezone: "America/New_York"
    )

    on_exit(fn -> Application.put_env(:the_gathering, TheGathering.Discord, original) end)
    start_supervised!({API, self()})

    scheduler =
      start_supervised!(
        {NewGameScheduler, name: nil, api: API, now: &API.now/0, interval: 60_000}
      )

    :sys.get_state(scheduler)
    %{scheduler: scheduler}
  end

  test "public command persists defaults, time, title, format and message identity", %{
    scheduler: server
  } do
    interaction =
      interaction(%{
        name: "newgame",
        options: [
          %{name: "start", value: "tomorrow 7pm"},
          %{name: "title", value: "Wednesday pod"},
          %{name: "format", value: "Pauper"}
        ]
      })

    assert :ok = NewGameCommand.respond(interaction, API, server, @now)
    assert_receive {:response, %{type: 5} = response}
    refute Map.has_key?(response, :data)
    assert_receive {:edit_response, %{content: "Preparing your game…"} = placeholder}
    refute Map.has_key?(placeholder, :components)

    assert_receive {:edit,
                    {222, 555, %{content: "", embeds: [embed], allowed_mentions: %{parse: []}}}}

    assert embed.title == "Wednesday pod"
    assert Enum.at(embed.fields, 0).value == "<t:1790290800:F> (<t:1790290800:R>)"
    assert Enum.at(embed.fields, 2).value == "Pauper"
    game = Repo.one!(ScheduledGame)

    assert {game.guild_id, game.channel_id, game.message_id, game.host_discord_id} ==
             {"333", "222", "555", "111"}

    assert game.start_at == ~U[2026-09-24 23:00:00Z]
    assert game.min_players == 3
    assert game.players == %{}
  end

  test "invalid time, bounds, DMs, and foreign guilds fail privately without creating queues", %{
    scheduler: server
  } do
    invalid = [
      interaction(%{name: "newgame", options: [%{name: "start", value: "yesterday"}]}),
      interaction(%{name: "newgame", options: [%{name: "start", value: "<t:1>"}]}),
      interaction(%{name: "newgame", options: [%{name: "min_players", value: 11}]}),
      interaction(%{name: "newgame", options: [%{name: "min_players", value: 1}]}),
      %{interaction(%{name: "newgame"}) | guild_id: nil},
      %{interaction(%{name: "newgame"}) | guild_id: 999}
    ]

    for event <- invalid do
      assert {:ok} = NewGameCommand.respond(event, API, server, @now)
      assert_receive {:response, %{type: 4, data: %{flags: 64}}}
    end

    assert Repo.aggregate(ScheduledGame, :count) == 0
    refute_receive {:edit_response, _}
  end

  test "join and leave edit original roster; repeat join updates name without duplication", %{
    scheduler: server
  } do
    game = queue(%{min_players: 4})
    assert {:ok, first} = NewGameScheduler.act(game.id, "join", actor("11"), server)
    assert_receive {:edit, {222, 555, %{embeds: [embed]}}}
    assert List.last(embed.fields) == %{name: "Players (1/10)", value: "<@11>"}
    API.set_now(DateTime.add(@now, 30))

    assert {:ok, second} =
             NewGameScheduler.act(
               game.id,
               "join",
               %{actor("11") | display_name: "New name"},
               server
             )

    assert map_size(second.players) == 1
    assert second.players["11"]["joined_at"] == first.players["11"]["joined_at"]
    assert second.players["11"]["display_name"] == "New name"
    assert {:ok, _} = NewGameScheduler.act(game.id, "join", actor("12"), server)
    assert {:ok, left} = NewGameScheduler.act(game.id, "leave", actor("11"), server)
    assert Map.keys(left.players) == ["12"]
    assert {:ok, ^left} = NewGameScheduler.act(game.id, "leave", actor("11"), server)
    assert_receive {:edit, {222, 555, %{embeds: [%{fields: [_, _, _, %{value: "<@12>"}]}]}}}
  end

  test "ten-player cap permits repeat joins and frees a seat on leave", %{scheduler: server} do
    game = queue(%{start_at: DateTime.add(@now, 3600)})

    for id <- 1..10,
        do: assert({:ok, _} = NewGameScheduler.act(game.id, "join", actor(to_string(id)), server))

    assert {:error, :full} = NewGameScheduler.act(game.id, "join", actor("11"), server)
    assert {:ok, full} = NewGameScheduler.act(game.id, "join", actor("3"), server)
    assert map_size(full.players) == 10
    assert full.status == "open"
    assert {:ok, _} = NewGameScheduler.act(game.id, "leave", actor("3"), server)
    assert {:ok, full} = NewGameScheduler.act(game.id, "join", actor("11"), server)
    assert map_size(full.players) == 10
  end

  test "minimum-met join starts immediately once and mentions only the joined players", %{
    scheduler: server
  } do
    game = queue(%{min_players: 2})
    assert {:ok, %{status: "open"}} = NewGameScheduler.act(game.id, "join", actor("11"), server)
    refute_receive {:create, _}
    assert {:ok, started} = NewGameScheduler.act(game.id, "join", actor("12"), server)
    assert started.status == "started"
    assert {:ok, _} = Ecto.UUID.cast(started.room_id)
    assert_receive {:create, {222, payload}}
    assert payload.content =~ TheGatheringWeb.Endpoint.url() <> "/table/" <> started.room_id
    assert payload.allowed_mentions == %{parse: [], users: ["11", "12"]}
    assert payload.enforce_nonce
    assert payload.nonce == "newgame:#{game.id}"

    assert_receive {:edit,
                    {222, 555,
                     %{
                       embeds: [%{description: "Your game is ready!" <> _}],
                       components: [%{components: buttons}]
                     }}}

    assert Enum.all?(buttons, & &1.disabled)
    assert {:ok, repeated} = NewGameScheduler.act(game.id, "leave", actor("11"), server)
    assert repeated.room_id == started.room_id
    assert repeated.players == started.players
    NewGameScheduler.sweep(server)
    refute_receive {:create, _}
  end

  test "scheduled queue starts at the boundary but not a second earlier", %{scheduler: server} do
    due = DateTime.add(@now, 60)
    game = queue(%{start_at: due, min_players: 2})
    for id <- ["11", "12"], do: NewGameScheduler.act(game.id, "join", actor(id), server)
    API.set_now(DateTime.add(due, -1))
    NewGameScheduler.sweep(server)
    assert Repo.get!(ScheduledGame, game.id).status == "open"
    refute_receive {:create, _}
    API.set_now(due)
    send(server, :tick)
    :sys.get_state(server)
    assert Repo.get!(ScheduledGame, game.id).status == "started"
    assert_receive {:create, _}
  end

  test "underfilled queues expire at deadline and cannot accept a late final join", %{
    scheduler: server
  } do
    due = DateTime.add(@now, 60)
    game = queue(%{start_at: due, min_players: 2})
    NewGameScheduler.act(game.id, "join", actor("11"), server)
    API.set_now(due)
    assert {:ok, expired} = NewGameScheduler.act(game.id, "join", actor("12"), server)
    assert expired.status == "expired"
    assert Map.keys(expired.players) == ["11"]
    assert expired.room_id == nil
    assert_receive {:edit, {222, 555, %{embeds: [%{description: "This game did not fill" <> _}]}}}
    refute_receive {:create, _}
  end

  test "boot reloads due work from DB and expires underfilled queues", %{scheduler: _server} do
    game = queue(%{start_at: DateTime.add(@now, -1)})
    stop_supervised!(NewGameScheduler)
    server = start_supervised!({NewGameScheduler, name: nil, api: API, now: &API.now/0})
    :sys.get_state(server)
    assert Repo.get!(ScheduledGame, game.id).status == "expired"
    assert_receive {:edit, {222, 555, %{components: [%{components: buttons}]}}}
    assert Enum.all?(buttons, & &1.disabled)
  end

  test "atomic status guard does not replace the UUID when given the same stale open row" do
    game = queue(%{start_at: @now, min_players: 2})
    stale = game |> change(players: players(["11", "12"])) |> Repo.update!()
    first = ScheduledGames.settle(stale, @now)
    second = ScheduledGames.settle(stale, @now)
    assert first.status == "started"
    assert second.room_id == first.room_id
    assert Repo.get!(ScheduledGame, game.id).room_id == first.room_id
  end

  test "host cancellation disables buttons and never starts; others cannot cancel", %{
    scheduler: server
  } do
    game = queue()
    assert {:error, :forbidden} = NewGameScheduler.act(game.id, "cancel", actor("12"), server)
    assert {:ok, cancelled} = NewGameScheduler.act(game.id, "cancel", actor("111"), server)
    assert cancelled.status == "cancelled"
    assert_receive {:edit, {222, 555, %{components: [%{components: buttons}]}}}
    assert Enum.all?(buttons, & &1.disabled)
    assert {:ok, unchanged} = NewGameScheduler.act(game.id, "join", actor("12"), server)
    assert unchanged.players == %{}
    refute_receive {:create, _}
  end

  test "parsed Nostrum administrator roles can cancel; Manage Guild alone cannot", %{
    scheduler: server
  } do
    game = queue()

    for {roles, expected} <- [{["445"], "open"}, {["444"], "cancelled"}] do
      event = interaction(%{custom_id: "newgame:#{game.id}:cancel"}, "12", roles)
      refute Map.has_key?(event.member, :permissions)
      assert {:ok, _} = NewGameCommand.respond(event, API, server, @now, API)
      assert Repo.get!(ScheduledGame, game.id).status == expected
      assert_receive {:response, %{type: 5, data: %{flags: 64}}}
      assert_receive {:edit_response, %{flags: 64}}
    end
  end

  test "button acknowledgements are private and mismatched message/channel/guild are rejected", %{
    scheduler: server
  } do
    game = queue()
    event = interaction(%{custom_id: "newgame:#{game.id}:join"})
    assert {:ok, _} = NewGameCommand.respond(event, API, server, @now)
    assert_receive {:response, %{type: 5, data: %{flags: 64}}}
    assert_receive {:edit_response, %{content: "You are on the roster.", flags: 64}}

    for actor <- [
          %{actor("12") | guild_id: "999"},
          %{actor("12") | channel_id: "999"},
          %{actor("12") | message_id: "999"}
        ] do
      assert {:error, :forbidden} = NewGameScheduler.act(game.id, "join", actor, server)
    end

    assert Map.keys(Repo.get!(ScheduledGame, game.id).players) == ["111"]
  end

  test "failed acknowledgement is not retried or published", %{scheduler: server} do
    API.fail([:response])

    ExUnit.CaptureLog.capture_log(fn ->
      assert {:error, :delivery_failed} =
               NewGameCommand.respond(interaction(%{name: "newgame"}), API, server, @now)
    end)

    assert Repo.one!(ScheduledGame).status == "cancelled"
    assert_receive {:response, %{type: 5}}
    refute_receive {:response, _}
    refute_receive {:edit_response, _}
  end

  test "restart retries failed notifications with same room and records announcement before edit",
       %{scheduler: server} do
    game = queue(%{min_players: 2})
    NewGameScheduler.act(game.id, "join", actor("11"), server)
    API.fail([:create])

    ExUnit.CaptureLog.capture_log(fn ->
      NewGameScheduler.act(game.id, "join", actor("12"), server)
    end)

    assert_receive {:create, {222, first}}
    room = Repo.get!(ScheduledGame, game.id).room_id
    stop_supervised!(NewGameScheduler)
    API.fail([:edit])

    server =
      ExUnit.CaptureLog.with_log(fn ->
        server = start_supervised!({NewGameScheduler, name: nil, api: API, now: &API.now/0})
        :sys.get_state(server)
        server
      end)
      |> elem(0)

    assert_receive {:create, {222, ^first}}
    assert Repo.get!(ScheduledGame, game.id).announcement_id == "999"
    assert Repo.get!(ScheduledGame, game.id).message_dirty
    NewGameScheduler.sweep(server)
    refute_receive {:create, _}
    final = Repo.get!(ScheduledGame, game.id)
    assert final.room_id == room
    refute final.message_dirty
  end

  test "queue rendering suppresses free-text mentions" do
    game = queue(%{title: "@everyone", format: "<@&444>"})
    payload = game |> NewGameMessage.render() |> Helpers.prepare_allowed_mentions()
    assert payload.allowed_mentions == %{parse: []}
  end

  test "first public queue edit can be recovered after restart", %{scheduler: server} do
    API.fail([:edit])

    ExUnit.CaptureLog.capture_log(fn ->
      assert :ok = NewGameCommand.respond(interaction(%{name: "newgame"}), API, server, @now)
    end)

    game = Repo.one!(ScheduledGame)
    assert game.message_id == "555"
    assert game.message_dirty
    assert_receive {:edit, {222, 555, _}}
    stop_supervised!(NewGameScheduler)
    server = start_supervised!({NewGameScheduler, name: nil, api: API, now: &API.now/0})
    :sys.get_state(server)
    refute Repo.get!(ScheduledGame, game.id).message_dirty
    assert_receive {:edit, {222, 555, %{components: [%{components: buttons}]}}}
    assert Enum.map(buttons, & &1.label) == ["Join", "Leave", "Cancel"]
    refute Enum.any?(buttons, & &1.disabled)
  end

  test "concurrent joins cannot overfill a scheduled queue", %{scheduler: server} do
    game = queue(%{start_at: DateTime.add(@now, 3600)})

    results =
      1..11
      |> Task.async_stream(
        fn id ->
          NewGameScheduler.act(game.id, "join", actor(to_string(id)), server)
        end,
        timeout: :infinity
      )
      |> Enum.map(fn {:ok, result} -> result end)

    assert Enum.count(results, &match?({:ok, _}, &1)) == 10
    assert Enum.count(results, &(&1 == {:error, :full})) == 1
    assert map_size(Repo.get!(ScheduledGame, game.id).players) == 10
  end

  test "guild owner can cancel without an explicit administrator role", %{scheduler: server} do
    game = queue()
    event = interaction(%{custom_id: "newgame:#{game.id}:cancel"}, "42")
    assert {:ok, _} = NewGameCommand.respond(event, API, server, @now, API)
    assert Repo.get!(ScheduledGame, game.id).status == "cancelled"
  end

  defp queue(attrs \\ %{}) do
    {:ok, game} = ScheduledGames.create(attrs, actor("111"))
    ScheduledGames.attach_message(game.id, "555")
  end

  defp actor(id),
    do: %{
      discord_id: id,
      display_name: "Player #{id}",
      guild_id: "333",
      channel_id: "222",
      message_id: "555",
      admin?: false
    }

  defp players(ids),
    do:
      Map.new(
        ids,
        &{&1, %{"display_name" => "Player #{&1}", "joined_at" => DateTime.to_iso8601(@now)}}
      )

  defp interaction(data, user \\ "111", roles \\ []) do
    data =
      Map.update(data, :options, [], fn options ->
        Enum.map(options, &Map.put(&1, :type, if(&1.name == "min_players", do: 4, else: 3)))
      end)

    Interaction.to_struct(%{
      guild_id: "333",
      channel_id: "222",
      message: %{id: "555"},
      member: %{
        user: %{id: user, username: "Name"},
        nick: "Nickname",
        roles: roles,
        permissions: "8"
      },
      data: data
    })
  end
end
