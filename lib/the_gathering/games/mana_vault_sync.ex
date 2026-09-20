defmodule TheGathering.Games.ManaVaultSync do
  @moduledoc false

  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Catalog
  alias TheGathering.Decklists.RemoteDecks
  alias TheGathering.Games.{Deck, Player}
  alias TheGathering.Repo

  def run(%User{} = user) do
    with %Player{} = player <- Repo.get_by(Player, user_id: user.id),
         true <- configured?(user),
         remote <- RemoteDecks.list(user),
         %{error: nil} <- Enum.find(remote.sources, &(&1.source == :manavault)) do
      decks = Enum.filter(remote.decks, &(&1.source == :manavault))
      sync(player, decks)
    else
      nil -> {:error, :bad_gateway}
      false -> {:error, :bad_request}
      %{error: _message} -> {:error, :bad_gateway}
    end
  end

  defp configured?(user),
    do: user.manavault_url not in [nil, ""] and user.manavault_api_key not in [nil, ""]

  defp sync(player, remote_decks) do
    summaries =
      remote_decks
      |> Enum.flat_map(& &1.commanders)
      |> Enum.map(&{nil, &1})
      |> Catalog.card_summaries()

    Repo.transaction(fn ->
      existing = Repo.all(from deck in Deck, where: deck.player_id == ^player.id)

      initial = %{
        by_url: Map.new(existing, &{&1.decklist_url, &1}),
        by_name: Map.new(existing, &{fold_name(&1.name), &1}),
        created: 0,
        updated: 0
      }

      result = Enum.reduce(remote_decks, initial, &upsert(&1, &2, player, summaries))
      %{created: result.created, updated: result.updated}
    end)
  end

  defp upsert(remote, state, player, summaries) do
    attrs = deck_attrs(remote, player.id, summaries)
    deck = Map.get(state.by_url, remote.url) || Map.get(state.by_name, fold_name(remote.name))

    case deck do
      nil ->
        case %Deck{} |> Deck.changeset(attrs) |> Repo.insert() do
          {:ok, created} -> put_deck(state, created, :created, state.created + 1)
          {:error, changeset} -> Repo.rollback(changeset)
        end

      existing ->
        case existing |> Deck.changeset(attrs) |> Repo.update() do
          {:ok, updated} -> put_deck(state, updated, :updated, state.updated + 1)
          {:error, changeset} -> Repo.rollback(changeset)
        end
    end
  end

  defp put_deck(state, deck, count_key, count) do
    state
    |> Map.put(count_key, count)
    |> put_in([:by_url, deck.decklist_url], deck)
    |> put_in([:by_name, fold_name(deck.name)], deck)
  end

  defp deck_attrs(remote, player_id, summaries) do
    [commander_name, partner_name] = Enum.take(remote.commanders ++ [nil, nil], 2)
    commander = Catalog.card_summary(summaries, nil, commander_name)
    partner = Catalog.card_summary(summaries, nil, partner_name)

    %{
      player_id: player_id,
      name: remote.name,
      commander_card_id: commander && commander.id,
      commander_name: (commander && commander.name) || commander_name,
      partner_card_id: partner && partner.id,
      partner_name: (partner && partner.name) || partner_name,
      color_identity: Enum.join(remote.color_identity),
      decklist_url: remote.url
    }
  end

  defp fold_name(name), do: name |> String.trim() |> String.downcase(:ascii)
end
