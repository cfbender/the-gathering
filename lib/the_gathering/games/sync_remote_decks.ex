defmodule TheGathering.Games.SyncRemoteDecks do
  @moduledoc """
  Folds a user's hosted decks (Moxfield, Archidekt, ManaVault) into their player's
  local deck list so the two never need separate views.

  Each remote deck is matched against the player's decks in order:

    1. the same `decklist_url` — an already-linked deck, refreshed from the host;
    2. the same name (case-insensitive) — linked and refreshed;
    3. the same commander pair, when that local deck has no link yet — linked
       and filled in, but its local name is kept.

  Anything unmatched becomes a new deck. Hosts that failed to list are skipped
  and reported in `errors`; their decks are left alone rather than guessed at.
  """

  import Ecto.Query

  alias TheGathering.Accounts.User
  alias TheGathering.Catalog
  alias TheGathering.Decklists.RemoteDecks
  alias TheGathering.Games.{Deck, Player}
  alias TheGathering.Repo

  @type result :: %{
          created: non_neg_integer(),
          updated: non_neg_integer(),
          errors: [%{source: atom(), error: String.t()}]
        }

  @spec run(User.t()) :: {:ok, result()} | {:error, :bad_request | Ecto.Changeset.t()}
  def run(%User{} = user) do
    with %Player{} = player <- Repo.get_by(Player, user_id: user.id),
         true <- configured?(user) do
      remote = RemoteDecks.list(user)
      failed = Enum.filter(remote.sources, & &1.error)
      failed_sources = MapSet.new(failed, & &1.source)
      decks = Enum.reject(remote.decks, &MapSet.member?(failed_sources, &1.source))

      with {:ok, counts} <- sync(player, decks) do
        {:ok, Map.put(counts, :errors, Enum.map(failed, &%{source: &1.source, error: &1.error}))}
      end
    else
      _ -> {:error, :bad_request}
    end
  end

  def configured?(%User{} = user) do
    user.moxfield_username not in [nil, ""] or
      user.archidekt_username not in [nil, ""] or
      (user.manavault_url not in [nil, ""] and user.manavault_api_key not in [nil, ""])
  end

  defp sync(player, remote_decks) do
    summaries =
      remote_decks
      |> Enum.flat_map(& &1.commanders)
      |> Enum.map(&{nil, &1})
      |> Catalog.card_summaries()

    Repo.transaction(fn ->
      existing =
        Repo.all(from deck in Deck, where: deck.player_id == ^player.id, order_by: deck.id)

      initial = %{
        by_url: Map.new(existing, &{&1.decklist_url, &1}),
        by_name: Map.new(existing, &{fold_name(&1.name), &1}),
        # Unlinked decks only: a deck that already points somewhere is never re-pointed
        # by a commander coincidence. Earliest deck wins when several share a commander.
        by_commanders:
          existing
          |> Enum.filter(&is_nil(&1.decklist_url))
          |> Enum.reverse()
          |> Map.new(&{commander_key(&1.commander_name, &1.partner_name), &1}),
        created: 0,
        updated: 0
      }

      result = Enum.reduce(remote_decks, initial, &upsert(&1, &2, player, summaries))
      %{created: result.created, updated: result.updated}
    end)
  end

  defp upsert(remote, state, player, summaries) do
    attrs = deck_attrs(remote, player.id, summaries)
    [commander_name, partner_name] = commander_names(remote)

    cond do
      deck = Map.get(state.by_url, remote.url) ->
        save(state, Deck.update_changeset(deck, attrs))

      deck = Map.get(state.by_name, fold_name(remote.name)) ->
        save(state, Deck.update_changeset(deck, attrs))

      deck = Map.get(state.by_commanders, commander_key(commander_name, partner_name)) ->
        save(state, Deck.update_changeset(deck, link_attrs(deck, attrs)))

      true ->
        save(state, Deck.changeset(%Deck{}, attrs))
    end
  end

  # Link an independently created deck: point it at the host and fill in what the
  # owner never recorded, without renaming what they call it.
  defp link_attrs(deck, attrs) do
    attrs
    |> Map.take([:decklist_url])
    |> maybe_fill(:commander_card_id, deck.commander_card_id, attrs.commander_card_id)
    |> maybe_fill(:partner_card_id, deck.partner_card_id, attrs.partner_card_id)
    |> maybe_fill(:color_identity, blank_to_nil(deck.color_identity), attrs.color_identity)
  end

  defp maybe_fill(attrs, key, nil, value) when not is_nil(value), do: Map.put(attrs, key, value)
  defp maybe_fill(attrs, _key, _current, _value), do: attrs

  defp save(state, changeset) do
    inserting? = is_nil(changeset.data.id)

    case Repo.insert_or_update(changeset) do
      {:ok, deck} -> remember(state, deck, if(inserting?, do: :created, else: :updated))
      {:error, changeset} -> Repo.rollback(changeset)
    end
  end

  defp remember(state, deck, count_key) do
    state
    |> Map.update!(count_key, &(&1 + 1))
    |> put_in([:by_url, deck.decklist_url], deck)
    |> put_in([:by_name, fold_name(deck.name)], deck)
    |> Map.update!(:by_commanders, fn by_commanders ->
      # Now linked, so it must not absorb a second remote deck with the same commander.
      Map.reject(by_commanders, fn {_key, candidate} -> candidate.id == deck.id end)
    end)
  end

  defp deck_attrs(remote, player_id, summaries) do
    [commander_name, partner_name] = commander_names(remote)
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

  defp commander_names(remote), do: Enum.take(remote.commanders ++ [nil, nil], 2)

  # Order-insensitive so "Thrasios + Tymna" and "Tymna + Thrasios" are one deck.
  defp commander_key(commander_name, partner_name) do
    [commander_name, partner_name]
    |> Enum.reject(&(&1 in [nil, ""]))
    |> Enum.map(&fold_name/1)
    |> Enum.sort()
  end

  defp fold_name(name), do: name |> String.trim() |> String.downcase(:ascii)

  defp blank_to_nil(""), do: nil
  defp blank_to_nil(value), do: value
end
