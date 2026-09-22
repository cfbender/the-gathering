defmodule TheGathering.Discord.WebGameDraft do
  @moduledoc "Private web handoff for any server member; reads never create game data."

  import Ecto.Query, only: [from: 2]

  alias TheGathering.{Accounts, Discord, Games, Repo}
  alias TheGathering.Discord.{PendingGame, ResultDraft, SaveWebGame}
  alias TheGathering.Games.Game

  def open(reference, winner_id, actor) do
    pending = pending(reference, actor.channel_id)

    with :ok <- authorize_server(pending, actor),
         :ok <- validate_winner(pending, winner_id) do
      now = DateTime.utc_now() |> DateTime.truncate(:second)
      Repo.delete_all(from d in ResultDraft, where: d.expires_at < ^now)

      Repo.insert(%ResultDraft{
        pending_game_id: pending.id,
        discord_id: actor.discord_id,
        guild_id: actor.guild_id,
        channel_id: actor.channel_id,
        snapshot: snapshot(pending),
        expires_at: DateTime.add(now, 3600),
        data: %{
          "winner" => winner_id,
          "duration" => max(div(DateTime.diff(now, pending.played_at), 60), 1)
        }
      })
    end
  end

  def load(id, user) do
    with {:ok, id} <- Ecto.UUID.cast(id),
         %ResultDraft{} = draft <- Repo.get(ResultDraft, id),
         true <- is_nil(user.disabled_at),
         true <- user.role == "admin" or user.discord_id == draft.discord_id,
         :gt <- DateTime.compare(draft.expires_at, DateTime.utc_now()),
         %PendingGame{} = pending <- Repo.get(PendingGame, draft.pending_game_id),
         true <- draft.snapshot == snapshot(pending),
         false <- recorded?(pending) do
      {:ok, draft, pending}
    else
      _ -> {:error, :not_found}
    end
  end

  def preview(id, user) do
    with {:ok, draft, pending} <- load(id, user) do
      identities =
        Enum.map(players(pending), &%{name: &1.display_name, discord_id: &1.discord_id})

      resolutions = Games.preview_player_resolutions(identities)

      seats =
        Enum.zip_with(identities, resolutions, fn identity, resolution ->
          %{
            discord_id: identity.discord_id,
            player_id: resolution.player && resolution.player.id,
            player_name: resolution.name
          }
        end)

      {:ok,
       %{
         id: draft.id,
         external_id: pending.external_id,
         played_at: pending.played_at,
         duration_minutes: draft.data["duration"],
         winner_discord_id: draft.data["winner"],
         seats: seats
       }}
    end
  end

  def save(id, user, attrs), do: SaveWebGame.run(id, user, attrs)
  def players(pending), do: Discord.pending_report(pending).players

  defp pending("", channel_id), do: Discord.latest_pending_in_channel(channel_id)

  defp pending(reference, _channel_id) do
    id = reference |> String.upcase() |> String.trim_leading("#") |> String.trim_leading("SB")
    Discord.get_pending_by_external_id("spellbot:SB#{id}")
  end

  defp authorize_server(nil, _actor), do: {:error, :not_found}
  defp authorize_server(_pending, %{discord_id: ""}), do: {:error, :forbidden}
  defp authorize_server(_pending, %{guild_id: ""}), do: {:error, :forbidden}

  defp authorize_server(pending, actor) do
    configured = Application.get_env(:the_gathering, Discord, [])[:guild_id]
    account = Accounts.get_user_by_discord_id(actor.discord_id)

    cond do
      actor.guild_id != pending.guild_id ->
        {:error, :forbidden}

      configured not in [nil, ""] and to_string(configured) != actor.guild_id ->
        {:error, :forbidden}

      account != nil and account.disabled_at != nil ->
        {:error, :forbidden}

      recorded?(pending) ->
        {:error, :not_found}

      true ->
        :ok
    end
  end

  defp validate_winner(_pending, nil), do: :ok

  defp validate_winner(pending, winner_id) do
    if Enum.any?(players(pending), &(&1.discord_id == winner_id)),
      do: :ok,
      else: {:error, :invalid_winner}
  end

  defp recorded?(pending),
    do:
      Repo.exists?(
        from g in Game, where: g.source == "discord" and g.external_id == ^pending.external_id
      )

  defp snapshot(pending),
    do:
      :crypto.hash(
        :sha256,
        :erlang.term_to_binary(
          {pending.players, pending.played_at, pending.guild_id, pending.channel_id}
        )
      )
end
