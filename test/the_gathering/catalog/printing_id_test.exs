defmodule TheGathering.Catalog.PrintingIdTest do
  use ExUnit.Case, async: true

  alias TheGathering.Catalog.PrintingId

  test "second halves use the same face identity as reverse sides" do
    id = "c2e085dd-a448-4f5a-9cfa-5c2034234e7c"
    assert PrintingId.parse(id) == {:ok, id, 0}
    assert PrintingId.parse(id <> "-1") == {:ok, id, 1}

    for suffix <- ["-0", "-2", "-01", "-1-1", "/1"] do
      assert PrintingId.parse(id <> suffix) == {:error, :bad_request}
    end
  end
end
