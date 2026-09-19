defmodule TheGatheringWeb.AppControllerTest do
  use TheGatheringWeb.ConnCase, async: true

  test "GET / serves the SPA shell with a CSRF token and the React entrypoint", %{conn: conn} do
    conn = get(conn, ~p"/")
    html = html_response(conn, 200)

    assert html =~ ~s(<div id="root"></div>)
    assert html =~ ~r/<meta name="csrf-token" content="[^"]+"/
    assert html =~ "assets/react/src/main.tsx"
    assert get_resp_header(conn, "cache-control") == ["no-cache, no-store, must-revalidate"]
  end

  test "client-side routes fall through to the SPA shell", %{conn: conn} do
    conn = get(conn, "/games/123/anything")

    assert html_response(conn, 200) =~ ~s(<div id="root"></div>)
  end

  test "requests proxied by the Vite dev server get relative script URLs", %{conn: conn} do
    direct = conn |> get(~p"/") |> html_response(200)
    assert direct =~ ~s(src="http://127.0.0.1:5173/@vite/client")

    proxied =
      conn
      |> put_req_header("x-the-gathering-vite-proxy", "1")
      |> get(~p"/")
      |> html_response(200)

    assert proxied =~ ~s(src="/@vite/client")
    refute proxied =~ "127.0.0.1:5173"
  end

  test "unknown API routes return JSON 404 rather than the shell", %{conn: conn} do
    conn = get(conn, "/api/does-not-exist")

    assert json_response(conn, 404) == %{"errors" => %{"detail" => "Not Found"}}
  end
end
