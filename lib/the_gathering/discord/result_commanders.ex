defmodule TheGathering.Discord.ResultCommanders do
  @moduledoc false

  alias TheGathering.Discord.CardChoice
  alias TheGathering.Games.ColorIdentity

  def put(data, player_id, fields) do
    choices = %{
      "commander" => CardChoice.resolve(fields["commander"], "Commander", :commander),
      "partner" => CardChoice.resolve(fields["partner"], "Partner", :partner)
    }

    Map.put(data, "commanders", Map.put(data["commanders"] || %{}, player_id, choices))
  end

  def choose(data, player_id, role, id) do
    choice = get_in(data, ["commanders", player_id, role]) || %{}

    with {:ok, chosen} <- CardChoice.choose(choice, id) do
      {:ok, put_in(data, ["commanders", player_id, role], chosen)}
    end
  end

  def validate(data, players) do
    Enum.reduce_while(players, {:ok, %{}}, fn player, {:ok, acc} ->
      case deck(get_in(data, ["commanders", player.discord_id])) do
        :unchanged ->
          {:cont, {:ok, acc}}

        {:ok, attrs} ->
          {:cont, {:ok, Map.put(acc, player.discord_id, attrs)}}

        {:error, error} ->
          {:halt, {:error, "#{player.display_name}: #{error} Open Commanders to fix it."}}
      end
    end)
  end

  defp deck(nil), do: :unchanged

  defp deck(choices) do
    with {:ok, commander} <- CardChoice.card(choices["commander"], "Commander"),
         {:ok, partner} <- CardChoice.card(choices["partner"], "Partner") do
      deck_attrs(commander, partner)
    end
  end

  defp deck_attrs(nil, nil), do: {:ok, nil}
  defp deck_attrs(nil, _partner), do: {:error, "Choose a commander before adding a partner."}

  defp deck_attrs(%{id: id}, %{id: id}),
    do: {:error, "Commander and partner must be different cards."}

  defp deck_attrs(commander, partner) do
    colors =
      Enum.flat_map(Enum.reject([commander, partner], &is_nil/1), &(&1.color_identity || []))

    {:ok,
     %{
       commander_card_id: commander.id,
       commander_name: commander.name,
       partner_card_id: partner && partner.id,
       partner_name: partner && partner.name,
       color_identity: colors |> Enum.join() |> ColorIdentity.canonical()
     }}
  end
end
