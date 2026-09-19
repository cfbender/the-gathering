defmodule TheGathering.Discord.TrackerTest do
  use ExUnit.Case

  alias TheGathering.Discord.{Command, GameReport, Tracker}

  defmodule TestSink do
    @behaviour TheGathering.Discord.Sink

    @impl true
    def handle_report(report) do
      send(Application.fetch_env!(:the_gathering, :discord_test_pid), {:report, report})
      :ok
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
  end

  test "does not let a non-player report themselves as winner" do
    Tracker.observe(report())
    assert_receive {:report, _report}

    assert {:error, :not_a_player} = Tracker.record_winner("SB12345", "999")
    refute_receive {:report, _report}
  end

  test "slash command gives ephemeral success and errors" do
    Tracker.observe(report())
    assert_receive {:report, _report}

    interaction = %{
      data: %{name: "won", options: [%{name: "game", value: "SB12345"}]},
      user: %{id: "111"},
      member: nil
    }

    assert %{type: 4, data: %{flags: 64, content: "Recorded you as the winner of SB12345."}} =
             Command.handle(interaction)

    assert_receive {:report, %GameReport{winner_discord_ids: ["111"]}}

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
