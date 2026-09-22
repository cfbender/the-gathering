defmodule TheGathering.Discord.TrackerTest do
  use TheGathering.DataCase, async: false

  alias TheGathering.Discord
  alias TheGathering.Discord.{Command, GameReport, PendingGame, Tracker}
  alias TheGathering.Discord.Sink.Games, as: GamesSink

  defmodule TestSink do
    @behaviour TheGathering.Discord.Sink

    @impl true
    def handle_report(report) do
      send(Application.fetch_env!(:the_gathering, :discord_test_pid), {:report, report})
      :ok
    end
  end

  defmodule FailingSink do
    @behaviour TheGathering.Discord.Sink

    @impl true
    def handle_report(%{winner_discord_ids: []}), do: :ok

    def handle_report(_report) do
      {:ok, _player} = TheGathering.Games.create_player(%{name: "Rolled Back"})
      {:error, :forced_failure}
    end
  end

  setup do
    Application.put_env(:the_gathering, :discord_test_pid, self())
    start_supervised!({Tracker, sink: TestSink})
    on_exit(fn -> Application.delete_env(:the_gathering, :discord_test_pid) end)
    :ok
  end

  test "dispatches observations and completed winner reports to the sink" do
    report = report()

    assert :ok = Tracker.observe(report)
    assert_receive {:report, ^report}

    assert {:ok, completed} = Tracker.record_winner("12345", "111")
    assert completed.winner_discord_ids == ["111"]
    assert completed.raw.winner_reported_by == "111"
    assert_receive {:report, ^completed}
    assert Discord.get_pending_by_external_id(report.external_id) == nil
  end

  test "staged reports survive a tracker restart" do
    report = report()
    assert :ok = Tracker.observe(report)
    assert_receive {:report, ^report}

    stop_supervised(Tracker)
    start_supervised!({Tracker, sink: TestSink})

    assert {:ok, completed} = Tracker.record_winner("SB12345", "111")
    assert completed.external_id == report.external_id
    assert_receive {:report, ^completed}
  end

  test "replaying a report updates its staged normalized data" do
    assert :ok = Tracker.observe(report())
    assert_receive {:report, _report}

    replay = %GameReport{
      report()
      | played_at: ~U[2025-06-16 12:00:00Z],
        players: [
          %{discord_id: "111", display_name: "Aria Updated", commander_name: "Alela"},
          %{discord_id: "222", display_name: "Bryn", commander_name: nil}
        ]
    }

    assert :ok = Tracker.observe(replay)
    assert_receive {:report, ^replay}

    pending = Discord.get_pending_by_external_id(replay.external_id)
    assert pending.played_at == replay.played_at
    assert Discord.pending_report(pending).players == replay.players
  end

  test "listing pending reports does not prune expired rows" do
    assert :ok = Tracker.observe(report())
    assert_receive {:report, _report}

    stale_at = DateTime.utc_now() |> DateTime.add(-31, :day) |> DateTime.truncate(:second)
    Repo.update_all(PendingGame, set: [updated_at: stale_at])

    assert [%PendingGame{}] = Discord.list_pending()
    assert %PendingGame{} = Discord.get_pending_by_external_id("spellbot:SB12345")

    assert :ok = Discord.prune_pending()
    assert Discord.list_pending() == []
  end

  test "pruning removes only reports older than 30 days" do
    assert :ok = Tracker.observe(report())
    assert_receive {:report, _report}

    stale_at = DateTime.utc_now() |> DateTime.add(-31, :day) |> DateTime.truncate(:second)
    Repo.update_all(PendingGame, set: [updated_at: stale_at])

    fresh = %GameReport{report() | external_id: "spellbot:SB20000"}
    assert {:ok, fresh_pending} = Discord.stage_report(fresh)

    assert :ok = Discord.prune_pending()
    assert Discord.get_pending_by_external_id("spellbot:SB12345") == nil
    assert Discord.get_pending_by_external_id(fresh.external_id).id == fresh_pending.id
  end

  test "does not let a non-player report themselves as winner" do
    Tracker.observe(report())
    assert_receive {:report, _report}

    assert {:error, :not_a_player} = Tracker.record_winner("SB12345", "999")
    refute_receive {:report, _report}
  end

  test "a failed bot resolution rolls back writes and leaves the pending game intact" do
    stop_supervised(Tracker)
    start_supervised!({Tracker, sink: FailingSink})

    assert :ok = Tracker.observe(report())

    assert {:error, {:sink_failed, :forced_failure}} =
             Tracker.record_winner("SB12345", "111")

    assert %PendingGame{} = Discord.get_pending_by_external_id("spellbot:SB12345")
    refute Repo.get_by(TheGathering.Games.Player, name: "Rolled Back")
  end

  test "without a game ID, completes the most recently started game in the channel" do
    older = report()
    other_channel = %GameReport{report() | external_id: "spellbot:SB30000", channel_id: "555"}

    newer = %GameReport{
      report()
      | external_id: "spellbot:SB20000",
        played_at: ~U[2025-06-15 18:00:00Z]
    }

    # Observe the newest game first so recency comes from played_at, not order.
    for observed <- [newer, other_channel, older] do
      assert :ok = Tracker.observe(observed)
      assert_receive {:report, ^observed}
    end

    assert {:ok, %GameReport{external_id: "spellbot:SB20000"}} =
             Tracker.record_latest_winner("444", "111")

    assert_receive {:report,
                    %GameReport{external_id: "spellbot:SB20000", winner_discord_ids: ["111"]}}

    assert {:error, :not_a_player} = Tracker.record_latest_winner("444", "999")
    assert {:error, :no_game_in_channel} = Tracker.record_latest_winner("666", "111")
  end

  test "without a game ID, skips a re-staged game that already has a winner" do
    stop_supervised(Tracker)
    start_supervised!({Tracker, sink: GamesSink})

    older = report()

    newer = %GameReport{
      report()
      | external_id: "spellbot:SB20000",
        played_at: ~U[2025-06-15 18:00:00Z]
    }

    assert :ok = Tracker.observe(older)
    assert :ok = Tracker.observe(newer)

    assert {:ok, %GameReport{external_id: "spellbot:SB20000"}} =
             Tracker.record_winner("SB20000", "111")

    assert :ok = Tracker.observe(newer)
    assert %PendingGame{} = Discord.get_pending_by_external_id(newer.external_id)
    assert [pending] = Discord.list_pending()
    assert pending.external_id == older.external_id

    assert {:ok, completed} = Tracker.record_latest_winner("444", "111")
    assert completed.external_id == older.external_id
  end

  test "without a game ID, reports no game when every staged game already has a winner" do
    stop_supervised(Tracker)
    start_supervised!({Tracker, sink: GamesSink})

    observed = report()
    assert :ok = Tracker.observe(observed)

    assert {:ok, %GameReport{external_id: "spellbot:SB12345"}} =
             Tracker.record_winner("SB12345", "111")

    assert :ok = Tracker.observe(observed)
    assert %PendingGame{} = Discord.get_pending_by_external_id(observed.external_id)
    assert Discord.list_pending() == []
    assert {:error, :no_game_in_channel} = Tracker.record_latest_winner("444", "111")
  end

  test "slash command without options uses the invoking channel" do
    Tracker.observe(report())
    assert_receive {:report, _report}

    interaction = %{
      type: 2,
      guild_id: 333,
      data: %{name: "won", options: nil},
      channel_id: 444,
      user: %{id: "222"},
      member: nil
    }

    assert %{type: 9, data: %{title: "Game details"}} = Command.handle(interaction)
    refute_receive {:report, _}
    assert Discord.get_pending_by_external_id("spellbot:SB12345")

    assert %{data: %{content: content}} = Command.handle(%{interaction | channel_id: 999})
    assert content =~ "haven't seen an unfinished SpellBot game"
  end

  test "slash command opens a modal and gives ephemeral errors" do
    Tracker.observe(report())
    assert_receive {:report, _report}

    interaction = %{
      type: 2,
      guild_id: 333,
      data: %{name: "won", options: [%{name: "game", value: "SB12345"}]},
      channel_id: 444,
      user: %{id: "111"},
      member: nil
    }

    assert %{type: 9} = Command.handle(interaction)
    refute_receive {:report, _}

    unknown = put_in(interaction, [:data, :options, Access.at(0), :value], "SB99999")

    assert %{data: %{flags: 64, content: content}} = Command.handle(unknown)
    assert content =~ "haven't seen"
  end

  defp report do
    %GameReport{
      external_id: "spellbot:SB12345",
      source: "discord",
      played_at: ~U[2025-06-15 15:08:43Z],
      guild_id: "333",
      channel_id: "444",
      players: [
        %{discord_id: "111", display_name: "Aria", commander_name: nil},
        %{discord_id: "222", display_name: "Bryn", commander_name: nil}
      ],
      winner_discord_ids: [],
      raw: %{}
    }
  end
end
