defmodule TheGathering.Stats do
  @moduledoc """
  Read-only statistics derived from games and their normalized seats.

  Every win/loss/draw figure uses all games. Figures built from data a playgroup
  may only have started recording later (seat positions, duration, turns, MVP
  cards) use only games played on or after the administrator's
  `detailed_stats_from` date; each payload reports that date so the UI can label
  those figures.
  """

  import Ecto.Query

  alias TheGathering.{Accounts, Catalog}
  alias TheGathering.Games.{ColorIdentity, Deck, Game, GamePlayer, Player}
  alias TheGathering.Repo

  def overview(params \\ %{}) do
    games = games(params)
    seats = Enum.flat_map(games, & &1.seats)
    cutoff = detailed_stats_from()
    detailed = detailed_games(games, cutoff)
    detailed_seats = Enum.flat_map(detailed, & &1.seats)
    card_art = card_art(seats)

    %{
      detailed_stats_from: cutoff,
      games_count: length(games),
      average_duration_minutes: average(detailed, & &1.duration_minutes),
      average_turns: average(detailed, & &1.turns),
      leaderboard: grouped_records(seats, & &1.player, & &1.player_id),
      games_by_month:
        games
        |> Enum.group_by(&Calendar.strftime(&1.played_at, "%Y-%m"))
        |> Enum.map(fn {month, rows} -> %{month: month, games: length(rows)} end)
        |> Enum.sort_by(& &1.month),
      seat_win_rates:
        grouped_records(detailed_seats, &%{id: &1.seat, name: "Seat #{&1.seat}"}, & &1.seat),
      color_win_rates:
        seats
        |> Enum.reject(&is_nil(&1.deck))
        |> grouped_records(
          &%{
            id: ColorIdentity.canonical(&1.deck.color_identity),
            name: ColorIdentity.name(&1.deck.color_identity)
          },
          &ColorIdentity.canonical(&1.deck.color_identity)
        ),
      commanders:
        seats
        |> Enum.reject(&is_nil(&1.deck))
        |> grouped_records(
          &%{
            id: &1.deck.commander_name,
            name: &1.deck.commander_name,
            art_crop_url:
              Catalog.art_crop_url(
                card_art,
                &1.deck.commander_card_id,
                &1.deck.commander_name
              )
          },
          fn seat ->
            seat.deck.commander_name
          end
        )
        |> Enum.take(8),
      recent_games: games |> Enum.take(6) |> Enum.map(&recent_game/1)
    }
  end

  def player(player_id, params \\ %{}) do
    with %Player{} = player <- Repo.get(Player, player_id) do
      games = games(params, player_id: player.id)
      seats = Enum.map(games, &Enum.find(&1.seats, fn seat -> seat.player_id == player.id end))
      results = seats |> Enum.reverse() |> Enum.map(& &1.result)
      cutoff = detailed_stats_from()

      detailed_seats =
        games
        |> detailed_games(cutoff)
        |> Enum.map(&Enum.find(&1.seats, fn seat -> seat.player_id == player.id end))

      card_art = card_art(detailed_seats)

      %{
        detailed_stats_from: cutoff,
        player: %{id: player.id, name: player.name},
        record: record(seats),
        streaks: streaks(results),
        recent_form: seats |> Enum.take(10) |> Enum.map(& &1.result),
        win_rate_over_time: cumulative_win_rate(games, player.id),
        decks:
          seats
          |> Enum.reject(&is_nil(&1.deck))
          |> grouped_records(& &1.deck, & &1.deck_id),
        head_to_head: head_to_head(games, player.id),
        seat_win_rates:
          grouped_records(detailed_seats, &%{id: &1.seat, name: "Seat #{&1.seat}"}, & &1.seat),
        favorite_seat: favorite_seat(detailed_seats),
        best_seat: best_seat(detailed_seats),
        mvp_cards: mvp_cards(detailed_seats, card_art)
      }
    end
  end

  def deck(deck_id, params \\ %{}) do
    with %Deck{} = deck <- Repo.get(Deck, deck_id) |> Repo.preload(:player) do
      games = games(params, deck_id: deck.id)
      seats = Enum.map(games, &Enum.find(&1.seats, fn seat -> seat.deck_id == deck.id end))
      cutoff = detailed_stats_from()
      detailed = detailed_games(games, cutoff)

      %{
        detailed_stats_from: cutoff,
        deck: entity(deck),
        player: entity(deck.player),
        record: record(seats),
        average_duration_minutes: average(detailed, & &1.duration_minutes),
        average_turns: average(detailed, & &1.turns),
        opponents: deck_opponents(games, deck.id),
        recent_games: games |> Enum.take(10) |> Enum.map(&recent_game(&1, deck.id)),
        win_rate_over_time: cumulative_win_rate(games, deck.player_id, deck.id)
      }
    end
  end

  defp games(params, filters \\ []) do
    Game
    |> maybe_date_from(value(params, :date_from))
    |> maybe_date_to(value(params, :date_to))
    |> maybe_player(filters[:player_id])
    |> maybe_deck(filters[:deck_id])
    |> order_by([game], desc: game.played_at, desc: game.id)
    |> preload(seats: [:player, :deck])
    |> Repo.all()
  end

  defp detailed_stats_from, do: Accounts.get_settings().detailed_stats_from

  defp detailed_games(games, nil), do: games

  defp detailed_games(games, %Date{} = cutoff) do
    Enum.filter(games, &(Date.compare(DateTime.to_date(&1.played_at), cutoff) != :lt))
  end

  defp grouped_records(rows, entity_fun, key_fun) do
    rows
    |> Enum.group_by(key_fun)
    |> Enum.map(fn {_key, group} -> Map.merge(entity(entity_fun.(hd(group))), record(group)) end)
    |> Enum.sort_by(&{-&1.games, -&1.win_rate, String.downcase(&1.name)})
  end

  defp record(rows) do
    wins = Enum.count(rows, &(&1.result == "win"))
    losses = Enum.count(rows, &(&1.result == "loss"))
    draws = Enum.count(rows, &(&1.result == "draw"))
    games = length(rows)

    %{games: games, wins: wins, losses: losses, draws: draws, win_rate: percentage(wins, games)}
  end

  defp cumulative_win_rate(games, player_id, deck_id \\ nil) do
    games
    |> Enum.reverse()
    |> Enum.reduce({[], 0, 0}, fn game, {points, wins, total} ->
      seat =
        Enum.find(game.seats, fn seat ->
          seat.player_id == player_id and (is_nil(deck_id) or seat.deck_id == deck_id)
        end)

      wins = wins + if(seat.result == "win", do: 1, else: 0)
      total = total + 1

      point = %{
        date: Date.to_iso8601(DateTime.to_date(game.played_at)),
        win_rate: percentage(wins, total)
      }

      {[point | points], wins, total}
    end)
    |> elem(0)
    |> Enum.reverse()
  end

  defp streaks(results) do
    win_runs = results |> Enum.chunk_by(&(&1 == "win")) |> Enum.filter(&(hd(&1) == "win"))
    current = results |> Enum.reverse() |> Enum.take_while(&(&1 == "win")) |> length()

    %{
      current_wins: current,
      longest_wins: win_runs |> Enum.map(&length/1) |> Enum.max(fn -> 0 end)
    }
  end

  defp head_to_head(games, player_id) do
    games
    |> Enum.flat_map(fn game ->
      mine = Enum.find(game.seats, &(&1.player_id == player_id))

      game.seats
      |> Enum.reject(&(&1.player_id == player_id))
      |> Enum.map(fn opponent ->
        %{opponent: opponent.player, mine: mine.result, theirs: opponent.result}
      end)
    end)
    |> Enum.group_by(& &1.opponent.id)
    |> Enum.map(fn {_id, rows} ->
      opponent = hd(rows).opponent

      %{
        id: opponent.id,
        name: opponent.name,
        games: length(rows),
        wins: Enum.count(rows, &(&1.mine == "win")),
        losses: Enum.count(rows, &(&1.theirs == "win")),
        draws: Enum.count(rows, &(&1.mine == "draw"))
      }
    end)
    |> Enum.sort_by(&{-&1.games, &1.name})
  end

  defp deck_opponents(games, deck_id) do
    games
    |> Enum.flat_map(fn game -> Enum.reject(game.seats, &(&1.deck_id == deck_id)) end)
    |> Enum.reject(&is_nil(&1.player))
    |> grouped_records(& &1.player, & &1.player_id)
  end

  defp mvp_cards(seats, card_art) do
    seats
    |> Enum.reject(&(is_nil(&1.mvp_card_name) or &1.mvp_card_name == ""))
    |> Enum.group_by(&{&1.mvp_card_id, &1.mvp_card_name})
    |> Enum.map(fn {{id, name}, rows} ->
      %{
        id: id,
        name: name,
        mentions: length(rows),
        art_crop_url: Catalog.art_crop_url(card_art, id, name)
      }
    end)
    |> Enum.sort_by(&{-&1.mentions, &1.name})
    |> Enum.take(8)
  end

  defp card_art(seats) do
    Catalog.art_crop_urls(
      Enum.flat_map(seats, fn seat ->
        deck_refs =
          if seat.deck,
            do: [
              {seat.deck.commander_card_id, seat.deck.commander_name},
              {seat.deck.partner_card_id, seat.deck.partner_name}
            ],
            else: []

        [{seat.mvp_card_id, seat.mvp_card_name} | deck_refs]
      end)
    )
  end

  defp favorite_seat([]), do: nil

  defp favorite_seat(seats),
    do: seats |> Enum.frequencies_by(& &1.seat) |> Enum.max_by(&elem(&1, 1)) |> elem(0)

  defp best_seat([]), do: nil

  defp best_seat(seats) do
    seats
    |> Enum.group_by(& &1.seat)
    |> Enum.max_by(fn {_seat, rows} -> {record(rows).win_rate, length(rows)} end)
    |> elem(0)
  end

  defp recent_game(game, deck_id \\ nil) do
    winner = Enum.find(game.seats, &(&1.result == "win"))
    tracked = deck_id && Enum.find(game.seats, &(&1.deck_id == deck_id))

    %{
      id: game.id,
      played_at: game.played_at,
      duration_minutes: game.duration_minutes,
      turns: game.turns,
      result: tracked && tracked.result,
      winner: winner && entity(winner.player),
      players: length(game.seats)
    }
  end

  defp average(rows, fun) do
    values = rows |> Enum.map(fun) |> Enum.reject(&is_nil/1)
    if values == [], do: nil, else: Float.round(Enum.sum(values) / length(values), 1)
  end

  defp percentage(_part, 0), do: 0.0
  defp percentage(part, total), do: Float.round(part * 100 / total, 1)

  defp entity(%{id: id, name: name} = value),
    do:
      %{id: id, name: name}
      |> maybe_put(:commander_name, Map.get(value, :commander_name))
      |> maybe_put(:art_crop_url, Map.get(value, :art_crop_url))

  defp maybe_put(map, _key, nil), do: map
  defp maybe_put(map, key, value), do: Map.put(map, key, value)

  defp value(params, key), do: Map.get(params, key) || Map.get(params, Atom.to_string(key))

  defp maybe_date_from(query, nil), do: query

  defp maybe_date_from(query, value) do
    case Date.from_iso8601(value) do
      {:ok, date} ->
        where(query, [game], game.played_at >= ^DateTime.new!(date, ~T[00:00:00], "Etc/UTC"))

      _error ->
        query
    end
  end

  defp maybe_date_to(query, nil), do: query

  defp maybe_date_to(query, value) do
    case Date.from_iso8601(value) do
      {:ok, date} ->
        where(
          query,
          [game],
          game.played_at < ^DateTime.new!(Date.add(date, 1), ~T[00:00:00], "Etc/UTC")
        )

      _error ->
        query
    end
  end

  defp maybe_player(query, nil), do: query

  defp maybe_player(query, id),
    do:
      where(
        query,
        [game],
        game.id in subquery(
          from seat in GamePlayer, where: seat.player_id == ^id, select: seat.game_id
        )
      )

  defp maybe_deck(query, nil), do: query

  defp maybe_deck(query, id),
    do:
      where(
        query,
        [game],
        game.id in subquery(
          from seat in GamePlayer, where: seat.deck_id == ^id, select: seat.game_id
        )
      )
end
