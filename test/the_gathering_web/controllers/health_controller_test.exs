defmodule TheGatheringWeb.HealthControllerTest do
  use TheGatheringWeb.ConnCase, async: true

  test "GET /api/health reports the database as reachable", %{conn: conn} do
    conn = get(conn, ~p"/api/health")

    assert json_response(conn, 200) == %{"status" => "ok"}
  end
end
