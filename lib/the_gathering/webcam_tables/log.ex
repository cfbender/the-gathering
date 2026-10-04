defmodule TheGathering.WebcamTables.Log do
  @moduledoc """
  The table log: human-readable entries, newest first, kept with the room's
  durable state so every seat sees the same history and a reload restores it.

  An entry is `%{id, at, text}` plus optional merge metadata (`actor`, `kind`,
  `life`, `counter`, `roll`) and a `count` once rapid changes coalesce. Ids only
  grow, so the head always has the largest id; a merge updates its entry in place
  and keeps that entry's id.
  """

  @max_entries 200
  @merge_window_ms 5_000

  @doc """
  Adds `content` at `at` (milliseconds). It merges into the latest entry with the
  same actor and kind if that entry changed within five seconds, so players
  adjusting at the same time each keep one line. Other players' entries do not
  separate a merge; table-wide entries (no actor) do. Life and counters keep the
  original value, rolls keep every result.
  """
  def append(log, content, at) do
    next = Map.merge(content, %{id: next_id(log), at: at})

    case merge_index(log, next) do
      nil -> Enum.take([next | log], @max_entries)
      index -> List.update_at(log, index, &merge(&1, next))
    end
  end

  defp next_id([]), do: 1
  defp next_id([head | _log]), do: head.id + 1

  defp merge_index(log, %{kind: kind, actor: actor} = next)
       when not is_nil(kind) and not is_nil(actor) do
    log
    |> Enum.take_while(&(not is_nil(&1[:actor])))
    |> Enum.find_index(&(&1.actor == actor and &1[:kind] == kind))
    |> case do
      nil -> nil
      index -> if (next.at - Enum.at(log, index).at) in 0..@merge_window_ms, do: index
    end
  end

  defp merge_index(_log, _next), do: nil

  defp merge(previous, next) do
    merged = %{next | id: previous.id} |> Map.put(:count, Map.get(previous, :count, 1) + 1)

    cond do
      previous[:life] && next[:life] ->
        life = %{next.life | from: previous.life.from}
        %{merged | life: life, text: life_text(life)}

      previous[:counter] && next[:counter] ->
        counter = %{next.counter | from: previous.counter.from}
        %{merged | counter: counter, text: counter_text(counter)}

      previous[:roll] && next[:roll] ->
        roll = %{next.roll | results: previous.roll.results ++ next.roll.results}

        %{
          merged
          | roll: roll,
            text: roll.prefix <> Enum.map_join(roll.results, ", ", &to_string/1)
        }

      true ->
        merged
    end
  end

  def rematch, do: %{text: "Rematch: back to setup with the same seats"}
  def joined(name), do: %{text: "#{name} joined the table"}
  def left(name), do: %{text: "#{name} left the table"}

  def monarch(nil), do: %{text: "The monarch left the table"}
  def monarch(holder), do: %{text: "#{holder.player_name} took the monarch"}

  def monarch(%{peer_id: peer_id} = holder, %{peer_id: peer_id}), do: monarch(holder)

  def monarch(holder, actor),
    do: %{text: "#{actor.player_name} gave #{holder.player_name} the monarch"}

  @doc "The line for a new seat order; `started?` is whether the clock was already running."
  def seat_order(true = _shuffled, _started?), do: %{text: "Seat order randomized"}
  def seat_order(false, false), do: %{text: "Game started in seat order"}
  def seat_order(false, true), do: %{text: "Seat order changed"}

  def elimination(seat, true),
    do: %{text: "#{seat.player_name} was eliminated", actor: seat.peer_id, kind: "eliminated"}

  def elimination(seat, false),
    do: %{
      text: "#{seat.player_name} was restored to the game",
      actor: seat.peer_id,
      kind: "restored"
    }

  def roll(%{kind: "dice", sides: sides} = roll),
    do: roll_entry(roll, "dice:#{sides}", "#{roll.player_name} rolled a d#{sides}: ")

  def roll(%{kind: "coin"} = roll),
    do: roll_entry(roll, "coin", "#{roll.player_name} flipped a coin: ")

  defp roll_entry(roll, kind, prefix) do
    %{
      text: prefix <> to_string(roll.result),
      actor: roll.actor,
      kind: kind,
      roll: %{prefix: prefix, results: [roll.result]}
    }
  end

  @doc """
  Lines for what a seat changed about itself: deck, life, camera and counters.
  Elimination is logged where the room decides it. `seats` names commander
  damage sources.
  """
  def seat_changes(previous, next, seats) do
    name = next.player_name

    Enum.reject(
      [
        next[:deck_id] != previous[:deck_id] && next[:deck_name] &&
          %{text: "#{name} chose #{next.deck_name}", actor: next.peer_id, kind: "deck"},
        next.life != previous.life &&
          life_entry(next.peer_id, %{name: name, from: previous.life, to: next.life}),
        next.camera_off != previous.camera_off &&
          %{
            text: "#{name} turned their camera #{if next.camera_off, do: "off", else: "on"}",
            actor: next.peer_id,
            kind: "camera"
          }
      ],
      &(&1 in [false, nil])
    ) ++ counter_changes(previous, next, seats)
  end

  defp life_entry(actor, life),
    do: %{text: life_text(life), actor: actor, kind: "life", life: life}

  defp life_text(life), do: "#{life.name}: #{life.from} → #{life.to} life"

  defp counter_changes(previous, next, seats) do
    name = next.player_name

    casts =
      for commander <- keys(previous.commander_casts, next.commander_casts),
          do:
            {"#{commander} commander tax", Map.get(previous.commander_casts, commander, 0) * 2,
             Map.get(next.commander_casts, commander, 0) * 2}

    damage =
      for id <- keys(previous.commander_damage, next.commander_damage),
          before = Map.get(previous.commander_damage, id, %{}),
          after_ = Map.get(next.commander_damage, id, %{}),
          commander <- keys(before, after_),
          do:
            {"damage from #{source_name(seats, id)}'s #{commander}",
             Map.get(before, commander, 0), Map.get(after_, commander, 0)}

    for {label, from, to} <-
          [{"poison", previous.poison, next.poison}, {"rad", previous.rad, next.rad}] ++
            casts ++ damage ++ custom_counter_changes(previous, next),
        from != to do
      counter = %{prefix: "#{name} #{label}: ", from: from, to: to}

      %{
        text: counter_text(counter),
        actor: next.peer_id,
        kind: "counter:#{label}",
        counter: counter
      }
    end
  end

  defp counter_text(counter), do: "#{counter.prefix}#{counter.from} → #{counter.to}"

  # Shared custom counters are matched by id, so renaming one does not log a change and a
  # counter that stops being shared (or is removed) leaves nothing behind. Seats saved before
  # custom counters existed have none.
  defp custom_counter_changes(previous, next) do
    before = Map.new(Map.get(previous, :custom_counters, []), &{&1["id"], &1["value"]})

    for counter <- Map.get(next, :custom_counters, []),
        do: {counter["label"], Map.get(before, counter["id"], 0), counter["value"]}
  end

  defp keys(a, b), do: Enum.uniq(Map.keys(a) ++ Map.keys(b))

  defp source_name(seats, id) do
    case Enum.find(seats, &(to_string(&1.player_id) == id)) do
      nil -> "player #{id}"
      seat -> seat.player_name
    end
  end
end
