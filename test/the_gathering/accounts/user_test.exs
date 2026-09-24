defmodule TheGathering.Accounts.UserTest do
  use ExUnit.Case, async: true

  alias TheGathering.Accounts.User

  @react_src Path.expand("../../../assets/react/src", __DIR__)

  test "palette ids match the React picker and every palette has light and dark CSS" do
    picker_ids =
      Regex.scan(~r/\{ id: "(\w+)", label:/, File.read!(Path.join(@react_src, "lib/theme.tsx")),
        capture: :all_but_first
      )
      |> List.flatten()

    assert picker_ids == User.palettes()

    css_blocks =
      Regex.scan(
        ~r/\[data-palette="(\w+)"\]\[data-theme="(light|dark)"\]/,
        File.read!(Path.join(@react_src, "palettes.css")),
        capture: :all_but_first
      )
      |> MapSet.new(fn [palette, mode] -> {palette, mode} end)

    # Claret is the base daisyUI light/dark theme in app.css, so it has no override block.
    expected =
      for palette <- User.palettes() -- ["claret"],
          mode <- ~w(light dark),
          into: MapSet.new(),
          do: {palette, mode}

    assert css_blocks == expected
  end
end
