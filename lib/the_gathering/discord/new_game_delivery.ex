defmodule TheGathering.Discord.NewGameDelivery do
  @moduledoc "Retries durable message work without generating another room. Called by the scheduler only."
  require Logger
  alias TheGathering.Discord.NewGameMessage
  alias TheGathering.Repo

  def deliver(%{message_id: nil}, _api), do: :ok
  def deliver(%{message_dirty: false}, _api), do: :ok

  def deliver(game, api) do
    with {:ok, game} <- announce(game, api),
         {:ok, game} <- ping_maybe(game, api),
         {:ok, _message} <-
           api.edit(
             String.to_integer(game.channel_id),
             String.to_integer(game.message_id),
             NewGameMessage.render(game)
           ) do
      game |> Ecto.Changeset.change(message_dirty: false) |> Repo.update!()
      :ok
    else
      {:error, _reason} ->
        # Do not log API bodies or interaction tokens. Keep dirty for the next sweep.
        Logger.warning("Discord newgame #{game.id} notification failed; will retry")
        {:error, :delivery_failed}
    end
  end

  defp announce(%{status: "started", announcement_id: nil} = game, api) do
    with {:ok, message} <-
           api.create(String.to_integer(game.channel_id), NewGameMessage.announcement(game)) do
      game |> Ecto.Changeset.change(announcement_id: to_string(message.id)) |> Repo.update()
    end
  end

  defp announce(game, _api), do: {:ok, game}

  # Saved before the embed edit, like the announcement, so an edit failure does not re-ping.
  defp ping_maybe(
         %{status: "open", maybe_ping_id: nil, maybe_pinged_at: %DateTime{}, maybe: maybe} = game,
         api
       )
       when map_size(maybe) > 0 do
    with {:ok, message} <-
           api.create(String.to_integer(game.channel_id), NewGameMessage.maybe_ping(game)) do
      game |> Ecto.Changeset.change(maybe_ping_id: to_string(message.id)) |> Repo.update()
    end
  end

  defp ping_maybe(game, _api), do: {:ok, game}
end
