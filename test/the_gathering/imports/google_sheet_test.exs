defmodule TheGathering.Imports.GoogleSheetTest do
  use ExUnit.Case, async: true

  alias TheGathering.Imports.GoogleSheet

  @header "Date\tWinner\tDeck\tDaniel\tDan\tJesse\tWin Con\tOther Decks\tNotes"

  test "parses pasted TSV, dates, notes, aliases, and exact kill attribution" do
    payload = """
    K I L L S
    #{@header}
    1/2/25\tDaniel\tTidus\t0\t\t\tCombat\tMatt (Chatterfang); Drew (Merieke)\tcomma, slash / kept
    2025-01-03\tDan\tFaramir\t\t\t3\tCombo\tDaniel (Flubs), Jesse (Karlov)\tMisplaced kill
    """

    assert {:ok, [first, second]} = GoogleSheet.parse(payload)
    assert first.line == 3
    assert first.date == ~D[2025-01-02]
    assert first.notes == "comma, slash / kept"

    assert first.kill_counts == [
             %{player: "Daniel", kills: 0},
             %{player: "Dan", kills: 0},
             %{player: "Jesse", kills: 0}
           ]

    assert hd(first.seats).kills == 0
    assert Enum.at(first.seats, 1).kills == 0

    assert second.date == ~D[2025-01-03]

    assert second.kill_counts == [
             %{player: "Daniel", kills: 0},
             %{player: "Dan", kills: 0},
             %{player: "Jesse", kills: 3}
           ]

    assert Enum.find(second.seats, &(&1.player == "Jesse")).kills == 3
    refute Enum.any?(second.errors, &String.contains?(&1, "header"))
    refute first.key == second.key
  end

  test "parses CSV with quoted commas and all supported Other Deck separators" do
    payload = """
    Date,Winner,Deck,Drew,Win Con,Other Decks,Notes
    9/21/2025,Drew,Gisela,1,Combat,"Matt (Agatha), Dan (Faramir)","notes, commas/slashes"
    9/22/25,Drew,Gisela,,Combat,Drewson (Karlov) Matt (Tidus),ok
    9/23/25,Drew,Gisela,,Combat,Drewson(Gisela); ,ok
    """

    assert {:ok, [one, two, three]} = GoogleSheet.parse(payload)
    assert one.notes == "notes, commas/slashes"
    assert Enum.map(one.seats, & &1.player) == ["Drew", "Matt", "Dan"]
    assert Enum.map(two.seats, & &1.player) == ["Drew", "Drewson", "Matt"]
    assert Enum.map(three.seats, & &1.player) == ["Drew", "Drewson"]
  end

  test "reports row problems without dropping malformed residue" do
    payload = """
    Date\tWinner\tDeck\tDan\tWin Con\tOther Decks\tNotes
    nope\tDaniel\t\t-1\t\tRealty (Kenrith; Landon (Flubs)\t
    1/2/25\tDaniel\tTidus\t3\t\tMatt (A); Drew (B)\t
    1/2/25\tDaniel\tTidus\t\t\tDaniel (Other)\t
    """

    assert {:ok, [malformed, overflow, duplicate]} = GoogleSheet.parse(payload)
    assert Enum.any?(malformed.errors, &String.contains?(&1, "malformed text"))
    assert Enum.any?(malformed.errors, &String.contains?(&1, "missing a deck"))
    assert Enum.any?(malformed.errors, &String.contains?(&1, "nonnegative integer"))
    assert "Date is invalid." in malformed.errors
    assert Enum.any?(overflow.errors, &String.contains?(&1, "exceed"))
    assert "A raw player is listed more than once." in duplicate.errors
  end

  test "missing opponents errors and blank or N/A winners create draws" do
    payload = """
    Date\tWinner\tDeck\tWin Con\tOther Decks\tNotes
    1/1/25\tDrew\tKarlov\t\t\tOnly winner
    1/2/25\tN/A\t\t\tMatt (A); Drew (B)\tDraw
    1/3/25\t\t\t\tMatt (A); Drew (B)\tDraw
    """

    assert {:ok, [missing, na, blank]} = GoogleSheet.parse(payload)
    assert Enum.any?(missing.errors, &String.contains?(&1, "Other Decks"))

    for row <- [na, blank] do
      assert Enum.all?(row.seats, &(&1.result == "draw"))

      assert row.warnings == [
               "No winner: all listed players will be recorded as a draw. Notes do not change results."
             ]
    end
  end

  test "same date games differ by complete row and exact duplicates get occurrence keys" do
    row = "1/2/25\tDaniel\tTidus\t\tCombat\tMatt (A)\tFine"
    other = "1/2/25\tMatt\tA\t\tCombat\tDaniel (Tidus)\tFine"

    assert {:ok, [first, second, duplicate]} =
             GoogleSheet.parse(Enum.join([@header, row, other, row], "\n"))

    refute first.key == second.key
    assert duplicate.key == first.key <> "-2"
    assert Enum.any?(duplicate.warnings, &String.contains?(&1, "Duplicate row"))
  end

  test "rejects unparseable files and incomplete headers" do
    assert {:error, message} = GoogleSheet.parse("Date,Winner,Deck\n1/2/25,Drew,A")
    assert message =~ "header is missing"
    assert {:error, _message} = GoogleSheet.parse("not a sheet")
  end

  test "plain pasted quotes survive but malformed quoted exports are rejected" do
    header = "Date\tWinner\tDeck\tWin Con\tOther Decks\tNotes\n"

    assert {:ok, [row]} =
             GoogleSheet.parse(
               header <>
                 "2/21/26\tDrew\tTifa\tSwing Out\tDan (Voja); Matt (Cloud)\tTifa said \"It's Tifa'ing time\"\n"
             )

    assert row.notes == "Tifa said \"It's Tifa'ing time\""
    assert row.errors == []

    assert {:error, _} =
             GoogleSheet.parse(
               header <> "2/21/26\tDrew\tTifa\tSwing Out\tMatt (Cloud)\t\"Unclosed quote\n"
             )
  end
end
