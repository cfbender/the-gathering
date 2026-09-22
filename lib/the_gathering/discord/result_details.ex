defmodule TheGathering.Discord.ResultDetails do
  @moduledoc false

  alias TheGathering.Catalog
  alias TheGathering.Discord.{CardChoice, ResultCommanders}
  alias TheGathering.Games.WinCondition

  def with_mvp(data) do
    choice = CardChoice.resolve(data["mvp"], "MVP")

    Map.merge(data, %{
      "mvp" => choice["name"],
      "mvp_id" => choice["id"],
      "mvp_candidates" => choice["candidates"],
      "mvp_error" => choice["error"]
    })
  end

  def validate(data, players) do
    with true <- data["details_done"] == true,
         true <- Enum.all?(players, &Map.has_key?(data, "kills_#{&1.discord_id}")),
         true <- data["winner"] in Enum.map(players, & &1.discord_id),
         true <- data["win_condition"] in (WinCondition.keys() -- ["draw"]),
         {:ok, turns} <- number(data["turns"], "Turns", 1, 10_000),
         {:ok, duration} <- number(data["duration"], "Duration", 1, 100_000),
         {:ok, kills} <- kills(data, players),
         {:ok, commanders} <- ResultCommanders.validate(data, players),
         {:ok, card} <- mvp(data) do
      {:ok,
       %{
         turns: turns,
         duration_minutes: duration,
         win_condition: data["win_condition"],
         notes: data["notes"],
         kills: kills,
         commanders: commanders,
         mvp_card_id: card && card.id,
         mvp_card_name: card && card.name
       }}
    else
      false -> {:error, "Complete the game details and every kills page before saving."}
      error -> error
    end
  end

  defp kills(data, players) do
    Enum.reduce_while(players, {:ok, %{}}, fn player, {:ok, acc} ->
      case number(data["kills_#{player.discord_id}"], "#{player.display_name}'s kills", 0, 5) do
        {:ok, value} -> {:cont, {:ok, Map.put(acc, player.discord_id, value)}}
        error -> {:halt, error}
      end
    end)
  end

  defp number(value, _label, _min, _max) when value in [nil, ""], do: {:ok, nil}

  defp number(value, label, min, max) do
    case Integer.parse(value) do
      {n, ""} when n >= min and n <= max -> {:ok, n}
      _ -> {:error, "#{label} must be a whole number from #{min} to #{max}, or blank if unknown."}
    end
  end

  defp mvp(%{"mvp" => ""}), do: {:ok, nil}

  defp mvp(data) do
    case data["mvp_id"] && Catalog.get_card(data["mvp_id"]) do
      nil -> {:error, data["mvp_error"] || "Select an MVP card or leave it blank."}
      card -> {:ok, card}
    end
  end
end
