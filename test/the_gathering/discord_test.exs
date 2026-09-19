defmodule TheGathering.DiscordTest do
  use ExUnit.Case, async: true

  alias TheGathering.Discord

  test "does not start when the bot token is absent or blank" do
    assert :ignore = Discord.start_link([])
    assert :ignore = Discord.start_link(bot_token: "")
  end
end
