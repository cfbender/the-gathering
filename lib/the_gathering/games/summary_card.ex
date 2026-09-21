defmodule TheGathering.Games.SummaryCard do
  @moduledoc false

  alias TheGathering.Games.WinCondition

  require EEx

  def svg(game, images \\ %{}) do
    seats = Enum.sort_by(game.seats, & &1.seat)
    winner = Enum.find(seats, &(&1.result == "win"))
    body_height = max(length(seats) * 88, 380)
    notes = lines(game.notes || "No notes recorded.", 100, 3)
    names = lines(if(winner, do: winner.player.name, else: "A shared finish"), 23, 2)

    commanders =
      lines(if(winner, do: commanders(winner.deck), else: "No winner at this table"), 43, 2)

    commander_y = body_height + 72 - (length(commanders) - 1) * 23
    name_y = commander_y - 32 - (length(names) - 1) * 40

    template(%{
      game: game,
      seats: seats,
      winner: winner,
      images: images,
      body_height: body_height,
      row_height: body_height / length(seats),
      names: names,
      commanders: commanders,
      name_y: name_y,
      commander_y: commander_y,
      height: body_height + 228 + length(notes) * 24,
      notes: notes
    })
  end

  def description(game) do
    winner = Enum.find(game.seats, &(&1.result == "win"))
    result = if winner, do: "Winner: #{winner.player.name}", else: "Draw"

    "Game ##{game.id}. #{result}. #{WinCondition.label(game.win_condition)}. #{length(game.seats)} players."
    |> String.slice(0, 1024)
  end

  defp art(nil, _images, _which), do: nil

  defp art(deck, images, :commander),
    do: images[{deck.commander_card_id, deck.commander_name, deck.commander_printing_id}]

  defp art(deck, images, :partner),
    do: images[{deck.partner_card_id, deck.partner_name, deck.partner_printing_id}]

  defp commanders(nil), do: "Commander not recorded"

  defp commanders(deck),
    do: Enum.join(Enum.reject([deck.commander_name, deck.partner_name], &is_nil/1), " / ")

  defp escape(text) do
    text
    |> to_string()
    |> String.replace(~r/[\x00-\x08\x0B\x0C\x0E-\x1F]/u, "")
    |> Plug.HTML.html_escape()
  end

  defp truncate(text, limit) do
    if String.length(text) > limit, do: String.slice(text, 0, limit - 1) <> "…", else: text
  end

  defp lines(text, width, count) do
    words = text |> String.replace(~r/\s+/u, " ") |> String.trim() |> String.split(" ")

    words
    |> Enum.reduce([""], fn word, [line | rest] ->
      if String.length(line <> " " <> word) <= width,
        do: [String.trim(line <> " " <> word) | rest],
        else: [truncate(word, width), line | rest]
    end)
    |> Enum.reverse()
    |> Enum.reject(&(&1 == ""))
    |> then(fn all ->
      visible = Enum.take(all, count)

      if length(all) > count,
        do: List.update_at(visible, -1, &(truncate(&1, width - 1) <> "…")),
        else: visible
    end)
  end

  EEx.function_from_file(:defp, :template, Path.expand("summary_card.svg.eex", __DIR__), [
    :assigns
  ])
end
