defmodule TheGathering.Discord.SummaryCommandTest do
  use TheGatheringWeb.ConnCase, async: false

  alias Nostrum.Struct.Interaction
  alias TheGathering.{Accounts, AccountsFixtures, Games, Repo}
  alias TheGathering.Accounts.User
  alias TheGathering.Discord.SummaryCommand
  alias TheGathering.Games.Game

  setup do
    user = AccountsFixtures.user_fixture()
    user = user |> Ecto.Changeset.change(discord_id: "551122") |> Repo.update!()
    {:ok, alice} = Games.create_player(%{name: "Alice"})
    {:ok, bob} = Games.create_player(%{name: "Bob"})
    config = Application.get_env(:the_gathering, TheGathering.Discord)
    Application.put_env(:the_gathering, TheGathering.Discord, guild_id: "9090")
    on_exit(fn -> Application.put_env(:the_gathering, TheGathering.Discord, config || []) end)

    attrs = %{
      played_at: ~U[2026-09-20 20:00:00Z],
      win_condition: "combat_damage",
      notes: "A close finish",
      seats: [
        %{player_id: alice.id, seat: 1, result: "loss", kills: 0},
        %{player_id: bob.id, seat: 2, result: "win", kills: 1}
      ]
    }

    interaction = %Interaction{
      guild_id: 9090,
      user: %Nostrum.Struct.User{id: 551_122},
      member: %Nostrum.Struct.Guild.Member{user_id: 551_122},
      data: %{name: "summary", options: nil}
    }

    %{user: user, attrs: attrs, interaction: interaction}
  end

  test "latest uses played_at then ID, not creation order, and preloads seats", %{attrs: attrs} do
    assert {:error, :not_found} = Games.find_summary_game()
    {:ok, first} = Games.create_game(attrs)
    {:ok, latest} = Games.create_game(attrs)
    {:ok, older} = Games.create_game(%{attrs | played_at: ~U[2025-01-01 00:00:00Z]})
    assert older.id > latest.id
    assert {:ok, %Game{id: id, seats: seats}} = Games.find_summary_game()
    assert id == latest.id
    assert Enum.map(Enum.sort_by(seats, & &1.seat), & &1.player.name) == ["Alice", "Bob"]
    assert {:ok, %Game{id: first_id}} = Games.find_summary_game(to_string(first.id))
    assert first_id == first.id
  end

  test "SpellBot IDs are explicit, source-scoped and case-insensitive", %{attrs: attrs} do
    {:ok, local} = Games.create_game(attrs)

    {:ok, discord} =
      Games.find_or_create_game_by_external_id("discord", "spellbot:SB#{local.id}", attrs)

    {:ok, _import} = Games.find_or_create_game_by_external_id("csv", "spellbot:SB7654", attrs)
    assert {:ok, %Game{id: id}} = Games.find_summary_game(" #sb#{local.id} ")
    assert id == discord.id
    assert {:ok, %Game{id: local_id}} = Games.find_summary_game(to_string(local.id))
    assert local_id == local.id
    assert {:error, :not_found} = Games.find_summary_game("SB7654")

    for invalid <- ["../../etc/passwd", "123abc", "-1", String.duplicate("9", 100)] do
      assert {:error, :bad_request} = Games.find_summary_game(invalid)
    end
  end

  test "active Discord-linked members only, guild-only, with configured guild enforced",
       context do
    %{attrs: attrs, interaction: interaction, user: user} = context
    {:ok, game} = Games.create_game(attrs)
    assert {:ok, %Game{id: id}} = SummaryCommand.prepare(interaction)
    assert id == game.id
    assert {:error, :forbidden} = SummaryCommand.prepare(%{interaction | guild_id: nil})
    assert {:error, :forbidden} = SummaryCommand.prepare(%{interaction | guild_id: 8080})
    stranger = %{interaction | member: nil, user: %Nostrum.Struct.User{id: 998_877}}
    assert {:error, :forbidden} = SummaryCommand.prepare(stranger)
    {:ok, %User{}} = Accounts.update_user(user, %{disabled_at: DateTime.utc_now()})
    assert {:error, :forbidden} = SummaryCommand.prepare(interaction)
  end

  test "defers publicly before uploading PNG, includes alt text, link and mention suppression",
       context do
    {:ok, game} = Games.create_game(context.attrs)

    # Discord omits options for /summary and nests the invoking user under member.
    interaction =
      Interaction.to_struct(%{
        guild_id: "9090",
        member: %{user: %{id: "551122"}},
        data: %{name: "summary", type: 1}
      })

    assert interaction.data.options == nil
    assert {:ok, :sent} = SummaryCommand.respond(interaction, __MODULE__)
    assert_receive {:initial_response, %{type: 5}}
    assert_receive {:edited_response, response}
    assert response.allowed_mentions == %{parse: []}
    assert response.content =~ "/games/#{game.id}"
    assert [%{id: 0, filename: filename, description: description}] = response.attachments
    assert description =~ "Winner: Bob"
    assert [%{name: ^filename, body: png}] = response.files
    assert <<137, 80, 78, 71, 13, 10, 26, 10, _::binary>> = png
    assert Repo.aggregate(Game, :count) == 1
  end

  test "bad IDs and missing games return private errors without deferral", %{
    interaction: interaction
  } do
    for {options, expected} <- [
          {nil, "No recorded game"},
          {[%{name: "game", value: "oops"}], "Use a Gathering"}
        ] do
      assert :ok =
               SummaryCommand.respond(
                 %{interaction | data: %{name: "summary", options: options}},
                 __MODULE__
               )

      assert_receive {:initial_response, %{type: 4, data: %{flags: 64, content: content}}}
      assert content =~ expected
      refute_receive {:edited_response, _}
    end
  end

  test "failed acknowledgement is not retried and never uploads", context do
    {:ok, _} = Games.create_game(context.attrs)
    Process.put(:fail_summary_ack, true)

    log =
      ExUnit.CaptureLog.capture_log(fn ->
        assert {:error, :network} = SummaryCommand.respond(context.interaction, __MODULE__)
      end)

    assert log =~ "Discord /summary acknowledge failed: network"
    assert_receive {:initial_response, %{type: 5}}
    refute_receive {:initial_response, _}
    refute_receive {:edited_response, _}
  end

  test "upload failure logs the stage and numeric codes without exposing response data",
       context do
    {:ok, _} = Games.create_game(context.attrs)

    error = %Nostrum.Error.ApiError{
      status_code: 403,
      response: %{"code" => 50_013, "message" => "private response body"}
    }

    Process.put(:summary_upload_result, {:error, error})
    interaction = %{context.interaction | token: "private interaction token"}
    level = Logger.level()
    Logger.configure(level: :info)
    on_exit(fn -> Logger.configure(level: level) end)

    log =
      ExUnit.CaptureLog.capture_log([level: :info], fn ->
        assert {:error, ^error} = SummaryCommand.respond(interaction, __MODULE__)
      end)

    assert log =~ "Discord /summary acknowledge completed"
    assert log =~ "Discord /summary upload failed: HTTP 403, Discord code 50013"
    refute log =~ "private response body"
    refute log =~ interaction.token
    assert_receive {:initial_response, %{type: 5}}
    assert_receive {:edited_response, _}
    refute_receive {:edited_response, _}
  end

  test "authenticated PNG preview uses the same renderer and never exposes public history",
       context do
    {:ok, game} = Games.create_game(context.attrs)
    path = "/api/games/#{game.id}/summary"
    assert context.conn |> get(path) |> json_response(401)
    conn = context.conn |> log_in_user(context.user) |> get(path)
    assert response(conn, 200) =~ <<137, 80, 78, 71>>
    assert get_resp_header(conn, "content-type") == ["image/png"]
    assert get_resp_header(conn, "cache-control") == ["private, no-store"]
    conn = context.conn |> log_in_user(context.user) |> get("/api/games/nope/summary")
    assert json_response(conn, 400)
  end

  def create_response(_interaction, response) do
    send(self(), {:initial_response, response})
    if Process.get(:fail_summary_ack), do: {:error, :network}, else: :ok
  end

  def edit_response(_interaction, response) do
    send(self(), {:edited_response, response})
    Process.get(:summary_upload_result, {:ok, :sent})
  end
end
