defmodule TheGatheringWeb.API.FallbackControllerTest do
  use TheGatheringWeb.ConnCase, async: true

  alias TheGatheringWeb.API.FallbackController

  test "renders changeset errors per field with interpolated placeholders", %{conn: conn} do
    changeset =
      {%{}, %{name: :string, seats: :integer}}
      |> Ecto.Changeset.cast(%{"name" => "", "seats" => 1}, [:name, :seats])
      |> Ecto.Changeset.validate_required([:name])
      |> Ecto.Changeset.validate_number(:seats, greater_than_or_equal_to: 2)

    conn = FallbackController.call(conn, {:error, changeset})

    assert json_response(conn, 422) == %{
             "errors" => %{
               "name" => ["can't be blank"],
               "seats" => ["must be greater than or equal to 2"]
             }
           }
  end

  for {status, code, phrase} <- [
        {:bad_request, 400, "Bad Request"},
        {:unauthorized, 401, "Unauthorized"},
        {:forbidden, 403, "Forbidden"},
        {:not_found, 404, "Not Found"}
      ] do
    test "maps {:error, #{inspect(status)}} to #{code}", %{conn: conn} do
      conn = FallbackController.call(conn, {:error, unquote(status)})

      assert json_response(conn, unquote(code)) == %{
               "errors" => %{"detail" => unquote(phrase)}
             }
    end
  end

  test "API mutations without a CSRF token are rejected" do
    conn =
      Phoenix.ConnTest.build_conn()
      |> Plug.Conn.put_private(:plug_skip_csrf_protection, false)
      |> Plug.Conn.put_req_header("content-type", "application/json")

    assert_raise Plug.CSRFProtection.InvalidCSRFTokenError, fn ->
      post(conn, ~p"/api/anything", "{}")
    end
  end
end
