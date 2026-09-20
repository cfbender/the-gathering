defmodule TheGathering.Games.ResolvePlayer do
  @moduledoc false

  import Ecto.Changeset
  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Games.Player
  alias TheGathering.Repo

  def run(name, discord_id, opts \\ []) when is_binary(name) do
    discord_id = normalize_discord_id(discord_id)
    user_id = Keyword.get(opts, :user_id)

    case find_player(name, discord_id) do
      %Player{} = player -> link_user(player, user_id)
      nil -> create_player(name, discord_id, user_id)
    end
  end

  def preview(identities) when is_list(identities) do
    {resolutions, _reserved_names} =
      Enum.map_reduce(identities, MapSet.new(), fn identity, reserved_names ->
        name = Map.fetch!(identity, :name)
        discord_id = normalize_discord_id(Map.get(identity, :discord_id))

        case find_player(name, discord_id) do
          %Player{} = player ->
            {%{status: :matched, name: player.name, player: player}, reserved_names}

          nil ->
            available_name = available_name(name, discord_id, reserved_names)

            {%{status: :create, name: available_name, player: nil},
             MapSet.put(reserved_names, fold_name(available_name))}
        end
      end)

    resolutions
  end

  defp find_player(_name, discord_id) when is_binary(discord_id),
    do: Repo.get_by(Player, discord_id: discord_id)

  defp find_player(name, nil) do
    Repo.one(
      from player in Player,
        where: fragment("lower(?)", player.name) == ^fold_name(name)
    )
  end

  defp create_player(name, discord_id, user_id) do
    name = available_name(name, discord_id, MapSet.new())

    %Player{}
    |> Player.changeset(%{name: name})
    |> Player.put_discord_id(discord_id)
    |> Player.put_user(user_id)
    |> validate_user(user_id)
    |> Repo.insert()
  end

  defp link_user(player, nil), do: {:ok, player}
  defp link_user(%Player{user_id: user_id} = player, user_id), do: {:ok, player}

  defp link_user(%Player{user_id: nil} = player, user_id) do
    player
    |> Player.changeset(%{})
    |> Player.put_user(user_id)
    |> validate_user(user_id)
    |> Repo.update()
  end

  defp link_user(%Player{}, _user_id), do: {:error, :discord_identity_conflict}

  defp validate_user(changeset, nil), do: changeset

  defp validate_user(changeset, user_id) do
    if Repo.exists?(from user in User, where: user.id == ^user_id),
      do: changeset,
      else: add_error(changeset, :user_id, "does not exist")
  end

  defp available_name(name, nil, _reserved_names), do: normalize_name(name)

  defp available_name(name, _discord_id, reserved_names) do
    base = normalize_name(name)

    candidates =
      Stream.concat(
        [base],
        Stream.iterate(2, &(&1 + 1))
        |> Stream.map(&with_suffix(base, " (#{&1})"))
      )

    Enum.find(candidates, fn candidate ->
      folded = fold_name(candidate)
      not MapSet.member?(reserved_names, folded) and not name_taken?(folded)
    end)
  end

  defp name_taken?(folded_name) do
    Repo.exists?(
      from player in Player,
        where: fragment("lower(?)", player.name) == ^folded_name
    )
  end

  defp with_suffix(base, suffix),
    do: String.slice(base, 0, 100 - String.length(suffix)) <> suffix

  defp normalize_name(name), do: name |> String.trim() |> String.slice(0, 100)
  defp normalize_discord_id(value) when value in [nil, ""], do: nil
  defp normalize_discord_id(value) when is_binary(value), do: value
  defp fold_name(name), do: name |> String.trim() |> String.downcase(:ascii)
end
