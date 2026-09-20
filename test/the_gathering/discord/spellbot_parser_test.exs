defmodule TheGathering.Discord.SpellBotParserTest do
  use ExUnit.Case, async: true

  alias TheGathering.Discord.SpellBotParser

  @spellbot_id "725510263251402832"

  test "parses a scrubbed real SpellBot ready embed into a report" do
    assert {:ok, report} = SpellBotParser.parse(fixture(), @spellbot_id)

    assert report.external_id == "spellbot:SB12345"
    assert report.source == "discord"
    assert report.guild_id == "333333333333333333"
    assert report.channel_id == "444444444444444444"
    assert report.played_at == ~U[2025-06-15 15:08:43Z]
    assert report.winner_discord_ids == []

    assert report.players == [
             %{
               discord_id: "111111111111111111",
               display_name: "Aria",
               commander_name: nil
             },
             %{
               discord_id: "222222222222222222",
               display_name: "Bryn",
               commander_name: nil
             }
           ]

    assert report.raw.message_id == "999999999999999999"
  end

  test "rejects a lookalike embed from another author" do
    message = put_in(fixture(), ["author", "id"], "555555555555555555")

    assert {:error, :not_spellbot} = SpellBotParser.parse(message, @spellbot_id)
  end

  test "rejects malformed and unrelated messages without raising" do
    assert {:error, :not_spellbot} = SpellBotParser.parse(%{}, @spellbot_id)

    message = put_in(fixture(), ["embeds", Access.at(0), "title"], "Looking for players")
    assert {:error, :not_started_game} = SpellBotParser.parse(message, @spellbot_id)

    message =
      put_in(
        fixture(),
        ["embeds", Access.at(0), "fields", Access.at(2), "value"],
        "not a timestamp"
      )

    assert {:error, :invalid_started_at} = SpellBotParser.parse(message, @spellbot_id)
  end

  test "ignores SpellBot's embed-less placeholder and text messages" do
    # SpellBot defers its interactions (empty "thinking" message) and replies to
    # validation errors in plain text; neither carries a game embed.
    assert {:error, :no_embeds} =
             SpellBotParser.parse(%{fixture() | "embeds" => [], "content" => ""}, @spellbot_id)

    assert {:error, :no_embeds} =
             SpellBotParser.parse(
               %{fixture() | "embeds" => [], "content" => "You are already in a game."},
               @spellbot_id
             )

    assert {:error, :no_embeds} =
             SpellBotParser.parse(Map.delete(fixture(), "embeds"), @spellbot_id)
  end

  defp fixture do
    "test/fixtures/discord/spellbot_game_ready.json"
    |> File.read!()
    |> Jason.decode!()
  end
end
