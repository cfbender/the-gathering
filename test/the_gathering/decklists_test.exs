defmodule TheGathering.DecklistsTest do
  use ExUnit.Case, async: false

  alias TheGathering.Decklists
  alias TheGathering.Decklists.Cache

  @fixture_dir Path.expand("../support/fixtures/decklists", __DIR__)

  setup do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    on_exit(fn ->
      Application.delete_env(:the_gathering, :decklists_req_options)
      Application.delete_env(:the_gathering, :decklists_cache_ttl_ms)
    end)
  end

  test "parses and canonicalizes provider URL variants" do
    cases = [
      {"https://www.moxfield.com/decks/abc?x=1", :moxfield, "abc",
       "https://moxfield.com/decks/abc"},
      {"https://moxfield.com/decks/a-b_C/primer/", :moxfield, "a-b_C",
       "https://moxfield.com/decks/a-b_C"},
      {"https://archidekt.com/decks/123/some-slug", :archidekt, "123",
       "https://archidekt.com/decks/123"},
      {"https://www.archidekt.com/decks/456/?foo=bar", :archidekt, "456",
       "https://archidekt.com/decks/456"},
      {"https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx?view=grid", :manavault,
       "AbCdEfGhIjKlMnOpQrStUvWx",
       "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"},
      {"https://www.manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx/", :manavault,
       "AbCdEfGhIjKlMnOpQrStUvWx",
       "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"}
    ]

    for {input, source, id, canonical_url} <- cases do
      assert {:ok, %{source: ^source, id: ^id, canonical_url: ^canonical_url}} =
               Decklists.parse_url(input)
    end
  end

  test "treats ManaVault links as other when no MANAVAULT_URL is configured" do
    configured = Application.get_env(:the_gathering, Decklists)
    Application.delete_env(:the_gathering, Decklists)
    on_exit(fn -> Application.put_env(:the_gathering, Decklists, configured) end)

    url = "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"
    assert {:ok, %{source: :other, canonical_url: ^url}} = Decklists.parse_url(url)
    assert {:error, :unsupported_url} = Decklists.resolve(url)
  end

  test "returns other for valid unknown links and invalid_url for garbage" do
    assert {:ok, %{source: :other, canonical_url: "http://example.com/a?b=1"}} =
             Decklists.parse_url("http://example.com/a?b=1#section")

    assert {:error, :invalid_url} = Decklists.parse_url("not a URL")
    assert {:error, :invalid_url} = Decklists.parse_url("ftp://moxfield.com/decks/abc")
  end

  test "resolves a Moxfield deck with partner commanders" do
    stub_fixture("moxfield_partner.json")

    assert {:ok, deck} = Decklists.resolve("https://moxfield.com/decks/partners")
    assert deck.name == "Partner Commander"

    assert deck.commanders == [
             %{name: "Kraum, Ludevic's Opus"},
             %{name: "Malcolm, Keen-Eyed Navigator"}
           ]

    assert deck.color_identity == ~w(U R)
    assert deck.author == "Goodybarsco"
    assert deck.card_count == 100

    assert deck.cards == [
             %{
               name: "Kraum, Ludevic's Opus",
               quantity: 1,
               zone: :commander,
               printing_id: "5b4d8b79-7a17-4f07-9dd5-4bb3ee0d3a5d"
             },
             %{
               name: "Malcolm, Keen-Eyed Navigator",
               quantity: 1,
               zone: :commander,
               printing_id: "9d5b2c1e-3e77-4a4f-9d52-1b4a3c8e6f10"
             },
             %{
               name: "Island",
               quantity: 12,
               zone: :mainboard,
               printing_id: "a1b2c3d4-0000-4000-8000-000000000001"
             },
             %{
               name: "Sol Ring",
               quantity: 1,
               zone: :mainboard,
               printing_id: "7e0c2f04-1d50-4fcd-9f1c-3c2a1b0e9d8f"
             }
           ]
  end

  test "resolves an Archidekt deck with a Background" do
    stub_fixture("archidekt_background.json")

    assert {:ok, deck} = Decklists.resolve("https://archidekt.com/decks/24907541/slug")
    assert deck.commanders == [%{name: "Noble Heritage"}, %{name: "Wilson, Refined Grizzly"}]
    assert deck.color_identity == ~w(W G)
    assert deck.author == "Will3545"
    # The Maybeboard is excluded from the deck, so neither the count nor the list has Cultivate.
    assert deck.card_count == 100

    assert Enum.map(deck.cards, &{&1.name, &1.quantity, &1.zone}) == [
             {"Noble Heritage", 1, :commander},
             {"Wilson, Refined Grizzly", 1, :commander},
             {"Forest", 38, :mainboard},
             {"Other cards", 60, :mainboard}
           ]

    assert Enum.map(deck.cards, & &1.printing_id) == [
             "0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e01",
             "0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e02",
             "0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03",
             nil
           ]
  end

  test "resolves a ManaVault shared deck" do
    stub_fixture("manavault.json")

    url = "https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"
    assert {:ok, deck} = Decklists.resolve(url)
    assert deck.name == "Shared Deck"
    assert deck.commanders == [%{name: "Shorikai, Genesis Engine"}]
    assert deck.color_identity == ~w(W U)
    assert deck.author == nil
    assert deck.card_count == 100

    # `considering` is left out; the preferred printing wins over the fallback.
    assert deck.cards == [
             %{
               name: "Shorikai, Genesis Engine",
               quantity: 1,
               zone: :commander,
               printing_id: "b3a0e8d4-1f2c-4c4e-9a55-6f1d2e3c4b01"
             },
             %{
               name: "Sol Ring",
               quantity: 1,
               zone: :mainboard,
               printing_id: "b3a0e8d4-1f2c-4c4e-9a55-6f1d2e3c4b02"
             }
           ]
  end

  test "follows ManaVault deck-card pages" do
    parent = self()

    Req.Test.stub(__MODULE__, fn conn ->
      {:ok, body, conn} = Plug.Conn.read_body(conn)
      after_cursor = Jason.decode!(body)["variables"]["after"]
      send(parent, {:page, after_cursor})

      {edges, page_info} =
        case after_cursor do
          nil ->
            {[node("commander", "Shorikai, Genesis Engine")],
             %{"endCursor" => "c1", "hasNextPage" => true}}

          "c1" ->
            {[node("mainboard", "Sol Ring", 1), node("mainboard", "Island", 30)],
             %{"endCursor" => "c2", "hasNextPage" => false}}
        end

      Req.Test.json(conn, %{
        "data" => %{
          "deck" => %{
            "name" => "Paged",
            "cardCount" => 32,
            "commanderColorIdentity" => ~w(W U),
            "deckCards" => %{"pageInfo" => page_info, "edges" => edges}
          }
        }
      })
    end)

    url = "https://manavault.example.com/share/decks/PagedPagedPagedPagedPaged"
    assert {:ok, deck} = Decklists.resolve(url)
    assert_receive {:page, nil}
    assert_receive {:page, "c1"}

    assert Enum.map(deck.cards, &{&1.name, &1.quantity}) == [
             {"Shorikai, Genesis Engine", 1},
             {"Sol Ring", 1},
             {"Island", 30}
           ]

    assert deck.commanders == [%{name: "Shorikai, Genesis Engine"}]
  end

  test "maps missing and private upstream responses" do
    Req.Test.stub(__MODULE__, fn conn ->
      status = if String.contains?(conn.request_path, "private"), do: 403, else: 404
      Plug.Conn.send_resp(conn, status, "")
    end)

    assert {:error, :not_found} = Decklists.resolve("https://moxfield.com/decks/missing")
    assert {:error, :private} = Decklists.resolve("https://moxfield.com/decks/private")
  end

  test "caches successful resolutions but not failures" do
    parent = self()

    Req.Test.expect(__MODULE__, 1, fn conn ->
      send(parent, :requested)
      Req.Test.json(conn, fixture("moxfield_partner.json"))
    end)

    url = "https://moxfield.com/decks/cache-me"
    assert {:ok, first} = Decklists.resolve(url)
    assert {:ok, second} = Decklists.resolve(url)
    assert first == second
    assert_receive :requested
    refute_receive :requested
  end

  defp node(zone, name, quantity \\ 1) do
    %{"node" => %{"zone" => zone, "quantity" => quantity, "card" => %{"name" => name}}}
  end

  defp stub_fixture(name) do
    Req.Test.stub(__MODULE__, fn conn -> Req.Test.json(conn, fixture(name)) end)
  end

  defp fixture(name) do
    @fixture_dir
    |> Path.join(name)
    |> File.read!()
    |> Jason.decode!()
  end
end
