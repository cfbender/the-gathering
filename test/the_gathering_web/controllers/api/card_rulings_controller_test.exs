defmodule TheGatheringWeb.API.CardRulingsControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Catalog.RulingCache
  alias TheGathering.Repo

  setup :register_and_log_in_user

  setup do
    Application.put_env(:the_gathering, :scryfall_req_options, plug: {Req.Test, __MODULE__})
    Application.put_env(:the_gathering, :scryfall_search_limit, 1_000_000)

    on_exit(fn ->
      Application.delete_env(:the_gathering, :scryfall_req_options)
      Application.delete_env(:the_gathering, :scryfall_search_limit)
    end)

    :ok
  end

  test "fetches exact printing rulings and caches only the public fields", %{conn: conn} do
    ruling = %{"source" => "wotc", "published_at" => "2026-01-23", "comment" => "Draw a card."}

    Req.Test.expect(__MODULE__, fn request ->
      assert request.request_path == "/cards/printing-a/rulings"
      assert get_req_header(request, "user-agent") != []
      Req.Test.json(request, %{data: [Map.put(ruling, "oracle_id", "not-exposed")]})
    end)

    assert conn |> get(~p"/api/card-printings/printing-a/rulings") |> json_response(200) ==
             %{"data" => [ruling]}

    # No second upstream expectation: this request must use the DB cache.
    assert conn |> get(~p"/api/card-printings/printing-a/rulings") |> json_response(200) ==
             %{"data" => [ruling]}

    assert Repo.get!(RulingCache, "printing-a").rulings == [ruling]
  end

  test "caches empty results and refreshes expired results", %{conn: conn} do
    Repo.insert!(%RulingCache{
      id: "expired",
      rulings: [%{"comment" => "Outdated"}],
      fetched_at: DateTime.add(DateTime.utc_now(:second), -86_400)
    })

    Req.Test.expect(__MODULE__, fn request -> Req.Test.json(request, %{data: []}) end)

    assert conn |> get(~p"/api/card-printings/expired/rulings") |> json_response(200) == %{
             "data" => []
           }

    assert conn |> get(~p"/api/card-printings/expired/rulings") |> json_response(200) == %{
             "data" => []
           }

    assert Repo.get!(RulingCache, "expired").rulings == []
  end

  test "requires authentication and does not cache missing, malformed or failed responses", %{
    conn: conn
  } do
    assert build_conn() |> get(~p"/api/card-printings/secret/rulings") |> json_response(401)

    for {id, status, body, expected} <- [
          {"missing", 404, %{}, 404},
          {"unavailable", 503, %{}, 502},
          {"malformed", 200, %{"data" => "invalid"}, 502}
        ] do
      Req.Test.expect(__MODULE__, fn request ->
        Req.Test.json(%{request | status: status}, body)
      end)

      assert conn |> get("/api/card-printings/#{id}/rulings") |> json_response(expected)
      refute Repo.get(RulingCache, id)
    end

    Req.Test.expect(__MODULE__, fn request -> Req.Test.transport_error(request, :timeout) end)
    assert conn |> get(~p"/api/card-printings/timeout/rulings") |> json_response(502)
    refute Repo.get(RulingCache, "timeout")
  end
end
