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
  end

  test "resolves an Archidekt deck with a Background" do
    stub_fixture("archidekt_background.json")

    assert {:ok, deck} = Decklists.resolve("https://archidekt.com/decks/24907541/slug")
    assert deck.commanders == [%{name: "Noble Heritage"}, %{name: "Wilson, Refined Grizzly"}]
    assert deck.color_identity == ~w(W G)
    assert deck.author == "Will3545"
    assert deck.card_count == 100
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
