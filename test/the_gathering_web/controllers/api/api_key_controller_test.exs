defmodule TheGatheringWeb.API.ApiKeyControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  alias TheGathering.Accounts
  alias TheGathering.Accounts.ApiKey
  alias TheGathering.AccountsFixtures
  alias TheGathering.Repo

  setup :register_and_log_in_user

  test "creates a key, reveals the secret once, and stores only its digest", %{
    conn: conn,
    user: user
  } do
    created =
      conn
      |> post(~p"/api/session/api-keys", %{api_key: %{name: "  Stats script  "}})
      |> json_response(201)
      |> Map.fetch!("data")

    assert %{"name" => "Stats script", "prefix" => prefix, "token" => "tg_" <> _ = token} =
             created

    assert String.starts_with?(token, prefix)

    stored = Repo.get!(ApiKey, created["id"])
    assert stored.user_id == user.id
    assert stored.token_hash == :crypto.hash(:sha256, token)

    listed = conn |> get(~p"/api/session/api-keys") |> json_response(200) |> Map.fetch!("data")
    assert [%{"id" => id, "name" => "Stats script", "last_used_at" => nil}] = listed
    assert id == created["id"]
    refute Map.has_key?(hd(listed), "token")
  end

  test "requires a name", %{conn: conn} do
    assert %{"errors" => %{"name" => [_ | _]}} =
             conn
             |> post(~p"/api/session/api-keys", %{api_key: %{name: " "}})
             |> json_response(422)

    assert conn |> post(~p"/api/session/api-keys", %{}) |> json_response(400)
  end

  test "lists and revokes only the signed-in user's keys", %{conn: conn, user: user} do
    other = AccountsFixtures.user_fixture()
    {:ok, {_token, own_key}} = Accounts.create_api_key(user, %{"name" => "mine"})
    {:ok, {other_token, other_key}} = Accounts.create_api_key(other, %{"name" => "theirs"})

    assert [%{"name" => "mine"}] =
             conn |> get(~p"/api/session/api-keys") |> json_response(200) |> Map.fetch!("data")

    assert conn |> delete(~p"/api/session/api-keys/#{other_key.id}") |> json_response(404)
    assert Accounts.authenticate_api_key(other_token)

    assert conn |> delete(~p"/api/session/api-keys/#{own_key.id}") |> response(204)
    refute Repo.get(ApiKey, own_key.id)
  end

  test "requires a signed-in session" do
    assert build_conn() |> get(~p"/api/session/api-keys") |> json_response(401)
  end
end
