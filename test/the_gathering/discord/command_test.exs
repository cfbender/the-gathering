defmodule TheGathering.Discord.CommandTest do
  use ExUnit.Case, async: false

  alias TheGathering.Discord
  alias TheGathering.Discord.Command

  test "registers /log with an optional winner and removes only /won in each scope" do
    original = Application.get_env(:the_gathering, Discord, [])
    on_exit(fn -> Application.put_env(:the_gathering, Discord, original) end)

    for guild <- [nil, "333"] do
      Application.put_env(:the_gathering, Discord, guild_id: guild)
      assert {:ok, description} = Command.register(888, __MODULE__)
      assert description =~ "/log, /summary, and /newgame"
      assert_receive {:created, ^guild, %{name: "log", options: options}}
      assert %{type: 6, required: false} = Enum.find(options, &(&1.name == "winner"))
      assert_receive {:created, ^guild, %{name: "summary"}}

      assert_receive {:created, ^guild,
                      %{name: "newgame", dm_permission: false, options: options}}

      assert Enum.map(options, & &1.name) == ["start", "min_players", "title", "format"]
      assert %{type: 4, min_value: 2, max_value: 10, required: false} = Enum.at(options, 1)
      assert_receive {:deleted, ^guild, 123}
      refute_receive {:deleted, ^guild, 456}
    end
  end

  def create_guild_command(_app, guild, command) do
    send(self(), {:created, guild, command})
    {:ok, %{}}
  end

  def create_global_command(app, command), do: create_guild_command(app, nil, command)

  def guild_commands(_app, _guild),
    do: {:ok, [%{name: "won", id: 123}, %{name: "other", id: 456}]}

  def global_commands(app), do: guild_commands(app, nil)

  def delete_guild_command(_app, guild, id) do
    send(self(), {:deleted, guild, id})
    {:ok}
  end

  def delete_global_command(app, id), do: delete_guild_command(app, nil, id)
end
