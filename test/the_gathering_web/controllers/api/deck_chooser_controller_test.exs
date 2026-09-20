defmodule TheGatheringWeb.API.DeckChooserControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.AccountsFixtures
  alias TheGathering.Decklists.Cache
  alias TheGathering.Games

  setup %{conn: conn} do
    Cache.clear()
    Application.put_env(:the_gathering, :decklists_req_options, plug: {Req.Test, __MODULE__})

    # Personal ManaVault hosts are resolved and checked against private ranges before
    # every request, so give the fake host a public address.
    Application.put_env(:the_gathering, :decklists_dns_resolver, fn _host, family ->
      if family == :inet, do: {:ok, [{93, 184, 216, 34}]}, else: {:ok, []}
    end)

    on_exit(fn ->
      Application.delete_env(:the_gathering, :decklists_req_options)
      Application.delete_env(:the_gathering, :decklists_dns_resolver)
    end)

    user = AccountsFixtures.user_fixture()
    {:ok, player} = Games.create_player(%{name: "Chooser"}, user.id)
    %{conn: log_in_user(conn, user), user: user, player: player}
  end

  test "records skip and choose outcomes for the signed-in player's deck", ctx do
    {:ok, deck} =
      Games.create_deck(%{
        player_id: ctx.player.id,
        name: "Krenko",
        commander_name: "Krenko"
      })

    assert %{"data" => %{"skip_count" => 1}} =
             ctx.conn
             |> post(~p"/api/deck-chooser/#{deck.id}/outcomes", %{outcome: "skipped"})
             |> json_response(200)

    assert %{"data" => %{"skip_count" => 0}} =
             ctx.conn
             |> recycle()
             |> log_in_user(ctx.user)
             |> post(~p"/api/deck-chooser/#{deck.id}/outcomes", %{outcome: "played"})
             |> json_response(200)
  end

  test "explains when the user has no linked player", %{conn: conn} do
    unlinked = AccountsFixtures.user_fixture()

    assert %{"data" => %{"deck" => nil, "reason" => "player_not_linked"}} =
             conn
             |> log_in_user(unlinked)
             |> get(~p"/api/deck-chooser")
             |> json_response(200)
  end
end
