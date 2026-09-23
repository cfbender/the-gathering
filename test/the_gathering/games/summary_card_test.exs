defmodule TheGathering.Games.SummaryCardTest do
  use ExUnit.Case, async: true

  alias TheGathering.Catalog.CardImages
  alias TheGathering.Games.{Deck, Game, GamePlayer, Player, SummaryCard, SummaryImage}

  test "draw, six seats, partners, missing data and hostile text stay bounded and escaped" do
    game = %Game{
      id: 42,
      source: "discord",
      external_id: "spellbot:SB91",
      played_at: ~U[2026-09-21 00:00:00Z],
      notes: String.duplicate("A very long note & <unsafe> ", 100),
      seats:
        for(
          n <- 1..6,
          do: %GamePlayer{
            seat: n,
            result: "draw",
            kills: if(n == 1, do: 0),
            player: %Player{name: "<Alice & Bob>"},
            deck: if(n == 1, do: %Deck{commander_name: "Frodo", partner_name: "Sam"})
          }
        )
    }

    svg = SummaryCard.svg(game)
    assert svg =~ "DRAW"
    assert svg =~ "SB91"
    assert svg =~ "Frodo / Sam"
    assert svg =~ "&lt;Alice &amp; Bob&gt;"
    refute svg =~ "<unsafe>"
    assert svg =~ "Commander not recorded"
    assert svg =~ ">0</text>"
    assert svg =~ ">—</text>"
    assert svg =~ "height=\"828\""
    assert svg =~ "…"
    assert byte_size(svg) < 15_000
    assert SummaryCard.description(game) =~ "Draw"
  end

  test "renderer downloads the Scryfall source behind a catalog image-cache URL" do
    source =
      "https://cards.scryfall.io/art_crop/front/0/1/01234567-89ab-cdef-0123-456789abcdef.jpg?1700000000"

    cache_url = CardImages.url(source)
    assert cache_url =~ "/api/card-images?"
    assert CardImages.source(cache_url) == source
    assert CardImages.source(source) == source
    assert CardImages.source(nil) == nil
    assert CardImages.source("/api/card-images?other=1") == nil

    # Browser-facing cache URLs are relative and unauthenticated for the renderer; only the
    # unwrapped source passes the allowlist.
    assert SummaryImage.fetch_art(cache_url) == nil

    Req.Test.stub(__MODULE__, fn conn -> Plug.Conn.send_resp(conn, 200, <<255, 216, 255, 10>>) end)

    assert SummaryImage.fetch_art(CardImages.source(cache_url), plug: {Req.Test, __MODULE__}) ==
             "data:image/jpeg;base64," <> Base.encode64(<<255, 216, 255, 10>>)
  end

  test "art fetch only allows HTTPS Scryfall, raster formats, no redirects, capped response sizes" do
    for url <- [
          nil,
          "file:///etc/passwd",
          "http://cards.scryfall.io/card.jpg",
          "https://localhost/card.png",
          "https://cards.scryfall.io.evil.test/a",
          "https://user@cards.scryfall.io/a"
        ] do
      assert SummaryImage.fetch_art(url) == nil
    end

    for {status, body, accepted?} <- [
          {200, <<255, 216, 255, 10>>, true},
          {200, "<svg>bad</svg>", false},
          {302, "", false},
          {404, <<255, 216, 255, 10>>, false},
          {200, <<255, 216, 255>> <> String.duplicate("x", 2_000_000), false}
        ] do
      Req.Test.stub(__MODULE__, fn conn ->
        conn
        |> Plug.Conn.put_resp_header("location", "https://localhost/private")
        |> Plug.Conn.send_resp(status, body)
      end)

      result =
        SummaryImage.fetch_art("https://cards.scryfall.io/art_crop/test.jpg",
          plug: {Req.Test, __MODULE__}
        )

      if accepted?,
        do: assert(result == "data:image/jpeg;base64," <> Base.encode64(body)),
        else: assert(is_nil(result))
    end
  end
end
