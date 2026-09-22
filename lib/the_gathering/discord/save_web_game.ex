defmodule TheGathering.Discord.SaveWebGame do
  @moduledoc "Atomically resolves a fixed Discord roster, records its game, and consumes staging."

  alias TheGathering.Discord.WebGameDraft
  alias TheGathering.Games
  alias TheGathering.Repo

  @game_fields ~w(played_at turns duration_minutes win_condition notes)
  @seat_fields ~w(result kills mvp_card_id mvp_card_name)
  @deck_fields ~w(name commander_card_id commander_name partner_card_id partner_name color_identity decklist_url)a

  def run(id, user, attrs) do
    Repo.transaction(fn ->
      with {:ok, _draft, pending} <- WebGameDraft.load(id, user),
           {:ok, seats} <- seats(pending, attrs["seats"]),
           game_attrs =
             Map.merge(Map.take(attrs, @game_fields), %{
               "source" => "discord",
               "external_id" => pending.external_id,
               "seats" => seats
             }),
           {:ok, game} <- Games.create_game(game_attrs, user.id) do
        Repo.delete!(pending)
        game
      else
        {:error, reason} -> Repo.rollback(reason)
      end
    end)
  end

  defp seats(pending, seats) when is_list(seats) do
    roster = Map.new(WebGameDraft.players(pending), &{&1.discord_id, &1})
    ids = Enum.map(seats, &if(is_map(&1), do: &1["discord_id"]))

    if Enum.sort(ids) == Enum.sort(Map.keys(roster)) do
      resolve_seats(seats, roster)
    else
      {:error, :bad_request}
    end
  end

  defp seats(_pending, _seats), do: {:error, :bad_request}

  defp resolve_seats(seats, roster) do
    seats
    |> Enum.with_index(1)
    |> Enum.reduce_while({:ok, []}, fn {seat, index}, {:ok, acc} ->
      case seat(seat, roster[seat["discord_id"]], index) do
        {:ok, attrs} -> {:cont, {:ok, acc ++ [attrs]}}
        error -> {:halt, error}
      end
    end)
  end

  defp seat(attrs, identity, index) do
    with {:ok, player} <- Games.resolve_player(identity.display_name, identity.discord_id),
         {:ok, deck_id} <- deck(player, attrs) do
      {:ok,
       Map.merge(Map.take(attrs, @seat_fields), %{
         "player_id" => player.id,
         "deck_id" => deck_id,
         "seat" => index
       })}
    end
  end

  defp deck(player, %{"deck_id" => id}) when not is_nil(id) do
    with {:ok, id} <- Ecto.Type.cast(:id, id),
         %{player_id: player_id} <- Games.get_deck(id),
         true <- player_id == player.id do
      {:ok, id}
    else
      _ -> {:error, :bad_request}
    end
  end

  defp deck(player, %{"deck" => %{"name" => name} = attrs}) when is_binary(name) do
    attrs = Map.new(@deck_fields, &{&1, attrs[Atom.to_string(&1)]})
    with {:ok, deck} <- Games.find_or_create_deck(player, name, attrs), do: {:ok, deck.id}
  end

  defp deck(_player, attrs) do
    if attrs["deck"] in [nil, ""], do: {:ok, nil}, else: {:error, :bad_request}
  end
end
