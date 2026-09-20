defmodule TheGathering.Accounts.SignInWithDiscord do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Accounts.{ServerSettings, User}
  alias TheGathering.Games
  alias TheGathering.Repo

  @suffix_attempts 2..9

  def run(%{"sub" => discord_id} = claims) when is_binary(discord_id) do
    Repo.transaction(fn ->
      user = find_or_create_user(discord_id, claims)

      case Games.resolve_player(user.display_name, user.discord_id, user_id: user.id) do
        {:ok, _player} -> user
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  def run(_claims), do: {:error, :invalid_discord_user}

  defp find_or_create_user(discord_id, claims) do
    case Repo.get_by(User, discord_id: discord_id) do
      %User{disabled_at: disabled_at} when not is_nil(disabled_at) ->
        Repo.rollback(:disabled)

      %User{} = user ->
        user
        |> User.discord_profile_changeset(%{avatar_url: discord_avatar_url(claims)})
        |> Repo.update!()

      nil ->
        create_user(discord_id, claims)
    end
  end

  defp create_user(discord_id, claims) do
    unless registration_allowed?(), do: Repo.rollback(:registration_closed)

    username = available_username(claims["preferred_username"], discord_id)

    %User{}
    |> User.discord_changeset(%{
      username: username,
      display_name: claims["preferred_username"] || username,
      discord_id: discord_id,
      avatar_url: discord_avatar_url(claims)
    })
    |> Repo.insert!()
  end

  defp registration_allowed? do
    Repo.aggregate(User, :count) > 0 and Repo.get!(ServerSettings, 1).registration_enabled
  end

  defp available_username(preferred_username, discord_id) do
    base =
      preferred_username
      |> to_string()
      |> String.trim()
      |> String.downcase()
      |> String.replace(~r/[^a-z0-9_.-]/, "_")
      |> String.trim("_.-")
      |> String.slice(0, 32)

    base = if String.length(base) >= 3, do: base, else: "discord"

    taken? = fn candidate ->
      Repo.exists?(from user in User, where: user.username == ^candidate)
    end

    [base | Enum.map(@suffix_attempts, &"#{String.slice(base, 0, 38)}#{&1}")]
    |> Enum.find(fn candidate -> not taken?.(candidate) end)
    |> case do
      nil -> "#{String.slice(base, 0, 19)}_#{String.slice(discord_id, 0, 20)}"
      username -> username
    end
  end

  defp discord_avatar_url(%{"picture" => picture}) when is_binary(picture) do
    unless String.ends_with?(picture, "/nil"), do: picture
  end

  defp discord_avatar_url(_claims), do: nil
end
