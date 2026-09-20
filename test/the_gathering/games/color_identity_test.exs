defmodule TheGathering.Games.ColorIdentityTest do
  use ExUnit.Case, async: true

  alias TheGathering.Games.ColorIdentity

  test "canonical reorders letters into WUBRG order and drops noise" do
    assert ColorIdentity.canonical("GRW") == "WRG"
    assert ColorIdentity.canonical("gW") == "W"
    assert ColorIdentity.canonical("WW") == "W"
    assert ColorIdentity.canonical(nil) == ""
  end

  test "names guilds, shards, wedges, four-color, and five-color identities" do
    assert ColorIdentity.name("WU") == "Azorius"
    assert ColorIdentity.name("GB") == "Golgari"
    assert ColorIdentity.name("WRG") == "Naya"
    assert ColorIdentity.name("RGW") == "Naya"
    assert ColorIdentity.name("UBG") == "Sultai"
    assert ColorIdentity.name("WUBR") == "Yore"
    assert ColorIdentity.name("UBRG") == "Glint"
    assert ColorIdentity.name("WUBRG") == "Five-Color"
    assert ColorIdentity.name("R") == "Mono-Red"
    assert ColorIdentity.name("") == "Colorless"
  end
end
