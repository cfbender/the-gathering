defmodule TheGathering.Discord.WonCommand do
  @moduledoc false

  require Logger

  alias Nostrum.Api.Interaction
  alias TheGathering.Discord.{WonForm, WonReport}

  def respond(interaction, api \\ Interaction) do
    response = handle(interaction)

    case api.create_response(interaction, response) do
      {:ok} ->
        :ok

      {:error, _reason} ->
        Logger.error("Discord /won response failed (response type #{response.type})")
    end
  end

  def handle(%{type: 2, data: %{name: "won"}} = interaction) do
    option = Enum.find(interaction.data.options || [], &(&1.name == "game"))
    reference = if option, do: String.trim(option.value), else: ""

    case WonReport.open(reference, actor(interaction)) do
      {:ok, draft, pending} -> WonForm.modal(draft, pending, "details")
      {:error, error} -> WonForm.message(error)
    end
  end

  def handle(%{type: type, data: %{custom_id: "won:" <> rest}} = interaction)
      when type in [3, 5] do
    case String.split(rest, ":") do
      [id, action] -> handle_action(interaction, id, action)
      _ -> WonForm.message("Invalid result form. Run /won again.")
    end
  end

  def handle(_), do: WonForm.message("Invalid result form. Run /won again.")

  defp handle_action(%{type: 3} = interaction, id, action)
       when action in ["details", "kills0", "kills1"] do
    case WonReport.load(id, actor(interaction)) do
      {:ok, draft, pending} ->
        if action == "kills1" and length(WonReport.players(pending)) <= 5,
          do: WonForm.message("This game does not need another kills page."),
          else: WonForm.modal(draft, pending, action)

      {:error, error} ->
        WonForm.message(error)
    end
  end

  defp handle_action(interaction, id, action) do
    allowed =
      if interaction.type == 5,
        do: ~w(details kills0 kills1),
        else: ~w(winner condition mvp save cancel)

    if action in allowed do
      update_type =
        if interaction.type == 3 or Map.get(interaction, :message) != nil, do: 7, else: 4

      result = WonReport.act(id, action, values(interaction), actor(interaction))
      render_result(result, update_type)
    else
      WonForm.message("Invalid result action.")
    end
  end

  defp render_result({:ok, draft, pending}, type), do: WonForm.review(draft, pending, type)

  defp render_result({:invalid, draft, pending, error}, type),
    do: WonForm.review(draft, pending, type, error)

  defp render_result({:saved, external_id}, type) do
    WonForm.message(
      "Recorded #{String.replace_prefix(external_id, "spellbot:", "")}. Use /summary to share the recap.",
      type
    )
  end

  defp render_result(:cancelled, type),
    do: WonForm.message("Draft cancelled. The game is still pending.", type)

  defp render_result({:error, error}, _type), do: WonForm.message(error)

  defp values(%{type: 3, data: data}), do: %{"value" => List.first(data.values || [])}

  defp values(%{type: 5, data: data}) do
    for row <- data.components || [],
        field <- row.components || [],
        is_binary(field.custom_id),
        is_binary(field.value),
        into: %{},
        do: {field.custom_id, field.value |> String.trim() |> String.slice(0, 4000)}
  end

  defp actor(interaction) do
    %{
      discord_id: to_string(interaction.user && interaction.user.id),
      guild_id: to_string(interaction.guild_id),
      channel_id: to_string(interaction.channel_id)
    }
  end
end
