defmodule TheGathering.Discord.SpellBotParser do
  @moduledoc "Parses SpellBot's edited `Your game is ready!` Discord embed."

  alias TheGathering.Discord.GameReport

  @started_color 0xF8AE4A
  @footer ~r/^SpellBot Game ID: #(SB\d+) — Service: .+$/
  @player ~r/<@!?(\d+)>\s+\((.*)\)\s*$/
  @started_at ~r/^<t:(\d+)>$/

  @spec parse(map(), String.t() | nil) :: {:ok, GameReport.t()} | {:error, atom()}
  def parse(message, spellbot_user_id \\ nil) do
    spellbot_user_id =
      spellbot_user_id ||
        Application.get_env(:the_gathering, TheGathering.Discord, [])[:spellbot_user_id]

    with :ok <- validate_author(message, spellbot_user_id),
         {:ok, embed, spellbot_game_id} <- find_started_embed(value(message, :embeds, [])),
         {:ok, played_at} <- parse_played_at(embed),
         {:ok, players} <- parse_players(embed),
         true <- players != [] do
      {:ok,
       %GameReport{
         external_id: "spellbot:#{spellbot_game_id}",
         source: "discord",
         played_at: played_at,
         guild_id: string_id(value(message, :guild_id)),
         channel_id: string_id(value(message, :channel_id)),
         players: players,
         winner_discord_ids: [],
         raw: raw(message, embed)
       }}
    else
      false -> {:error, :missing_players}
      {:error, reason} -> {:error, reason}
    end
  end

  defp validate_author(message, spellbot_user_id) do
    author = value(message, :author, %{})

    if value(author, :bot) == true and
         string_id(value(author, :id)) == string_id(spellbot_user_id) do
      :ok
    else
      {:error, :not_spellbot}
    end
  end

  # SpellBot's deferred interaction placeholders and plain-text replies carry no
  # embeds; only the waiting/ready game post does.
  defp find_started_embed([]), do: {:error, :no_embeds}

  defp find_started_embed(embeds) when is_list(embeds) do
    Enum.find_value(embeds, {:error, :not_started_game}, fn embed ->
      footer = value(value(embed, :footer, %{}), :text, "")

      with "**Your game is ready!**" <- value(embed, :title),
           @started_color <- value(embed, :color),
           [_, game_id] <- Regex.run(@footer, footer) do
        {:ok, embed, game_id}
      else
        _ -> false
      end
    end)
  end

  defp find_started_embed(_), do: {:error, :no_embeds}

  defp parse_played_at(embed) do
    with {:ok, value} <- field_value(embed, "Started at"),
         [_, unix] <- Regex.run(@started_at, value),
         {unix, ""} <- Integer.parse(unix),
         {:ok, datetime} <- DateTime.from_unix(unix) do
      {:ok, datetime}
    else
      _ -> {:error, :invalid_started_at}
    end
  end

  defp parse_players(embed) do
    with {:ok, value} <- field_value(embed, "Players") do
      players =
        value
        |> String.split("\n", trim: true)
        |> Enum.flat_map(&parse_player/1)

      {:ok, players}
    end
  end

  defp parse_player(line) do
    case Regex.run(@player, line) do
      [_, discord_id, display_name] ->
        [%{discord_id: discord_id, display_name: display_name, commander_name: nil}]

      _ ->
        []
    end
  end

  defp field_value(embed, name) do
    case Enum.find(value(embed, :fields, []), &(value(&1, :name) == name)) do
      nil -> {:error, :missing_field}
      field -> {:ok, value(field, :value, "")}
    end
  end

  defp raw(message, embed) do
    %{
      message_id: string_id(value(message, :id)),
      author_id: string_id(value(value(message, :author, %{}), :id)),
      embed: %{
        title: value(embed, :title),
        description: value(embed, :description),
        color: value(embed, :color),
        footer: value(value(embed, :footer, %{}), :text),
        fields:
          Enum.map(value(embed, :fields, []), fn field ->
            %{name: value(field, :name), value: value(field, :value)}
          end)
      }
    }
  end

  defp value(map, key, default \\ nil)

  defp value(map, key, default) when is_map(map),
    do: Map.get(map, key, Map.get(map, to_string(key), default))

  defp value(_, _, default), do: default

  defp string_id(nil), do: ""
  defp string_id(id), do: to_string(id)
end
