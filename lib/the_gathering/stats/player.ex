defmodule TheGathering.Stats.Player do
  @moduledoc "Calculates one player's statistics view."

  alias TheGathering.{Accounts, Catalog, Repo}
  alias TheGathering.Games.Player
  alias TheGathering.Stats.{Commanders, Elo, Query, Records, Summaries}

  def get(player_id, params \\ %{}) do
    with %Player{} = player <- Repo.get(Player, player_id) do
      games = Query.games(params, player_id: player.id)
      mine = &Enum.find(&1.seats, fn seat -> seat.player_id == player.id end)
      seats = Enum.map(games, mine)
      results = seats |> Enum.reverse() |> Enum.map(& &1.result)
      cutoff = Accounts.get_settings().detailed_stats_from
      detailed = detailed_games(games, cutoff)
      detailed_seats = Enum.map(detailed, mine)
      card_art = card_art(detailed_seats)

      %{
        detailed_stats_from: cutoff,
        player: %{id: player.id, name: player.name},
        record: Records.record(seats),
        elo: elo(player.id, params),
        average_duration_minutes: Records.average(detailed, & &1.duration_minutes),
        average_turns: Records.average(detailed, & &1.turns),
        game_lengths: Summaries.game_lengths(detailed, mine),
        color_exposure: Records.color_exposure(seats),
        rival_commanders: rival_commanders(games, player.id),
        streaks: streaks(results),
        recent_form: seats |> Enum.take(10) |> Enum.map(& &1.result),
        win_rate_over_time:
          Records.cumulative_win_rate(
            games,
            &Enum.find(&1, fn seat -> seat.player_id == player.id end)
          ),
        decks:
          seats
          |> Enum.reject(&is_nil(&1.deck))
          |> Records.grouped_records(&Summaries.entity(&1.deck), & &1.deck_id),
        color_win_rates: Records.color_records(seats),
        head_to_head: head_to_head(games, player.id),
        seat_win_rates:
          Records.grouped_records(
            detailed_seats,
            &%{id: &1.seat, name: "Seat #{&1.seat}"},
            & &1.seat
          ),
        favorite_seat: favorite_seat(detailed_seats),
        best_seat: best_seat(detailed_seats),
        mvp_cards: mvp_cards(detailed_seats, card_art)
      }
    end
  end

  defp detailed_games(games, nil), do: games

  defp detailed_games(games, %Date{} = cutoff) do
    Enum.filter(games, &(Date.compare(DateTime.to_date(&1.played_at), cutoff) != :lt))
  end

  # Ratings depend on every game at the table, so the whole playgroup is replayed.
  defp elo(player_id, params) do
    ratings = params |> Query.games() |> Elo.ratings()

    case Enum.find_index(ratings, &(&1.id == player_id)) do
      nil ->
        nil

      index ->
        ratings
        |> Enum.at(index)
        |> Map.take([:rating, :peak, :history])
        |> Map.put(:rank, index + 1)
        |> Map.put(:players, length(ratings))
    end
  end

  # Opponents' commanders: how often each was faced, beat this player, or was beaten
  # by them. Mirror matches among the opponents count every seat.
  defp rival_commanders(games, player_id) do
    faced = games |> opponent_seats(player_id) |> Commanders.summarize()

    beaten =
      games
      |> Enum.filter(fn game ->
        Enum.any?(game.seats, &(&1.player_id == player_id and &1.result == "win"))
      end)
      |> opponent_seats(player_id)
      |> Commanders.summarize()
      |> Map.new(&{&1.id, &1.games})

    faced
    |> Enum.map(fn row ->
      row
      |> Map.take([:id, :name, :art_crop_url, :color_identity])
      |> Map.merge(%{faced: row.games, beat_me: row.wins, beaten: Map.get(beaten, row.id, 0)})
    end)
    |> Enum.sort_by(&{-&1.faced, -&1.beat_me, String.downcase(&1.name)})
  end

  defp opponent_seats(games, player_id) do
    for game <- games,
        seat <- game.seats,
        seat.player_id != player_id and not is_nil(seat.deck),
        do: %{seat | game: game}
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
    |> Enum.max_by(fn {_seat, rows} -> {Records.record(rows).win_rate, length(rows)} end)
    |> elem(0)
  end
end
