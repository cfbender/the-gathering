defmodule TheGathering.Discord.WonForm do
  @moduledoc false

  alias TheGathering.Discord.WonReport
  alias TheGathering.Games.WinCondition

  def modal(draft, _pending, "details") do
    inputs = [
      input(draft, "turns", "Turns (optional)", 1, 10),
      input(draft, "duration", "Duration in minutes (estimate; edit as needed)", 1, 10),
      input(draft, "mvp", "Winner's MVP card name (optional)", 1, 150),
      input(draft, "notes", "Game notes (optional)", 2, 4000)
    ]

    modal_response(draft, "details", "Game details", inputs)
  end

  def modal(draft, pending, "kills" <> page = action) when page in ["0", "1"] do
    inputs =
      pending
      |> WonReport.kills_page(String.to_integer(page))
      |> Enum.map(fn player ->
        input(
          draft,
          "kills_#{player.discord_id}",
          "#{player.display_name} — kills",
          1,
          2,
          "0–5; leave blank if unknown"
        )
      end)

    modal_response(draft, action, "Player kills · page #{String.to_integer(page) + 1}", inputs)
  end

  def commander_modal(draft, player) do
    choices = get_in(draft.data, ["commanders", player.discord_id])

    values =
      if choices do
        Map.new(choices, fn {role, choice} -> {role, choice["name"]} end)
      else
        %{"commander" => player.commander_name || "", "partner" => ""}
      end

    inputs = [
      input(%{draft | data: values}, "commander", "Commander (name or partial name)", 1, 150),
      input(%{draft | data: values}, "partner", "Partner / Background (optional)", 1, 150)
    ]

    modal_response(
      draft,
      "commander_#{player.discord_id}",
      short(player.display_name, 30) <> " · commander",
      inputs
    )
  end

  def commanders(draft, pending, type, player_id \\ nil) do
    players = WonReport.players(pending)
    choices = get_in(draft.data, ["commanders", player_id]) || %{}
    lines = Enum.map(players, &commander_line(draft, &1))

    errors =
      Enum.map(~w(commander partner), &get_in(choices, [&1, "error"])) |> Enum.reject(&is_nil/1)

    content = Enum.join(["**Player commanders** — not saved yet" | lines] ++ errors, "\n")

    matches =
      Enum.flat_map(~w(commander partner), fn role ->
        candidates = get_in(choices, [role, "candidates"]) || []

        if candidates == [],
          do: [],
          else: [
            select(
              draft,
              "#{role}_choice_#{player_id}",
              "Choose #{role}",
              Enum.map(candidates, &{&1["id"], &1["name"]}),
              nil
            )
          ]
      end)

    components =
      [
        select(
          draft,
          "player",
          "Choose a player to enter or edit commanders",
          Enum.map(players, &{&1.discord_id, &1.display_name}),
          nil
        )
      ] ++ matches ++ [row([button(draft, "review", "Back to review", 2)])]

    response(content, components, type)
  end

  defp commander_line(draft, player) do
    choices = get_in(draft.data, ["commanders", player.discord_id])

    names =
      if choices do
        Enum.map(~w(commander partner), &choice_name(choices[&1]))
        |> Enum.reject(&(&1 == ""))
        |> Enum.join(" + ")
      else
        player.commander_name || ""
      end

    "#{short(player.display_name, 32)}: #{short(if(names == "", do: "Not recorded", else: names), 160)}"
  end

  defp choice_name(%{"error" => nil, "name" => name}), do: name
  defp choice_name(choice), do: choice["name"] <> " (unresolved)"

  def review(draft, pending, type \\ 4, error \\ nil) do
    players = WonReport.players(pending)
    winner = Enum.find(players, &(&1.discord_id == draft.data["winner"]))

    kills =
      Enum.map_join(
        players,
        " · ",
        &"#{short(&1.display_name, 32)}: #{display(draft.data["kills_#{&1.discord_id}"])}"
      )

    content =
      Enum.join(
        Enum.reject(
          [
            "**Review #{String.replace_prefix(pending.external_id, "spellbot:", "")}** — not saved yet",
            error || draft.data["mvp_error"],
            "Winner: #{winner && short(winner.display_name, 100)} · #{WinCondition.label(draft.data["win_condition"])}",
            "Turns: #{display(draft.data["turns"])} · Minutes: #{display(draft.data["duration"])}",
            "MVP: #{short(draft.data["mvp"] || "", 150)}",
            "Kills: #{kills}",
            "Commander entries: #{map_size(draft.data["commanders"] || %{})} · use Commanders to review",
            "Notes: #{short(draft.data["notes"] || "", 500)}",
            "Use the dropdowns and buttons to finish. Blank kills mean unknown, not zero. Draft expires after one hour."
          ],
          &is_nil/1
        ),
        "\n"
      )

    winner_options =
      Enum.with_index(players, 1)
      |> Enum.map(fn {p, n} -> {p.discord_id, "#{n}. #{p.display_name}"} end)

    conditions = WinCondition.values() |> Enum.reject(fn {value, _} -> value == "draw" end)
    choices = draft.data["mvp_candidates"] || []

    mvp =
      if choices == [],
        do: [],
        else: [
          select(
            draft,
            "mvp",
            "Choose the MVP card",
            Enum.map(choices, &{&1["id"], &1["name"]}),
            draft.data["mvp_id"]
          )
        ]

    edit_buttons = [
      button(draft, "details", "Edit details", 2),
      button(draft, "kills0", "Player kills", 2),
      button(draft, "commanders", "Commanders", 2)
    ]

    edit_buttons =
      if length(players) > 5,
        do: edit_buttons ++ [button(draft, "kills1", "More player kills", 2)],
        else: edit_buttons

    components =
      [
        select(draft, "winner", "Winner", winner_options, draft.data["winner"]),
        select(draft, "condition", "Win condition", conditions, draft.data["win_condition"])
      ] ++
        mvp ++
        [
          row(edit_buttons),
          row([button(draft, "save", "Save game", 3), button(draft, "cancel", "Cancel", 4)])
        ]

    response(content, components, type)
  end

  def message(content, type \\ 4), do: response(content, [], type)

  defp response(content, components, type) do
    data = %{content: content, allowed_mentions: %{parse: []}, components: components}
    # Updating an existing private message preserves its visibility.
    data = if type == 4, do: Map.put(data, :flags, 64), else: data
    %{type: type, data: data}
  end

  defp modal_response(draft, action, title, inputs),
    do: %{
      type: 9,
      data: %{
        custom_id: id(draft, action),
        title: title,
        components: Enum.map(inputs, &row([&1]))
      }
    }

  defp input(draft, name, label, style, max_length, placeholder \\ "Optional") do
    %{
      type: 4,
      custom_id: name,
      label: String.slice(label, 0, 45),
      style: style,
      required: false,
      max_length: max_length,
      value: draft.data[name] || "",
      placeholder: placeholder
    }
  end

  defp select(draft, action, placeholder, options, selected) do
    row([
      %{
        type: 3,
        custom_id: id(draft, action),
        placeholder: placeholder,
        min_values: 1,
        max_values: 1,
        options:
          Enum.map(options, fn {value, label} ->
            %{label: String.slice(label, 0, 100), value: value, default: value == selected}
          end)
      }
    ])
  end

  defp button(draft, action, label, style),
    do: %{type: 2, custom_id: id(draft, action), label: label, style: style}

  defp row(components), do: %{type: 1, components: components}
  defp id(draft, action), do: "won:#{draft.id}:#{action}"
  defp display(value) when value in [nil, ""], do: "—"
  defp display(value), do: value
  defp short(text, limit), do: text |> String.slice(0, limit) |> String.replace(~r/[`*_~]/u, "")
end
