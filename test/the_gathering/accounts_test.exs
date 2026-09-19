defmodule TheGathering.AccountsTest do
  use TheGathering.DataCase

  alias TheGathering.Accounts

  @valid %{
    "username" => "Player.One",
    "display_name" => "Player One",
    "password" => "long-enough-password"
  }

  test "the first registration becomes admin and registration then closes" do
    assert %{allowed: true, bootstrap: true} = Accounts.registration_status()
    assert {:ok, user} = Accounts.register_user(@valid)
    assert user.role == "admin"
    assert user.username == "player.one"

    assert %{allowed: false, bootstrap: false} = Accounts.registration_status()

    assert {:error, :registration_closed} =
             Accounts.register_user(Map.put(@valid, "username", "another"))

    assert {:ok, _settings} = Accounts.update_settings(%{"registration_enabled" => true})

    assert {:error, :registration_closed} =
             Accounts.register_user(Map.put(@valid, "username", "password-member"))
  end

  test "usernames are unique after case normalization" do
    assert {:ok, _user} = Accounts.create_admin(@valid)

    assert {:error, changeset} =
             Accounts.create_user(%{
               "username" => "PLAYER.ONE",
               "display_name" => "Someone Else",
               "password" => "another-long-password",
               "role" => "member"
             })

    assert "has already been taken" in errors_on(changeset).username
  end

  test "disabled users cannot authenticate" do
    assert {:ok, _admin} = Accounts.create_admin(@valid)

    assert {:ok, user} =
             Accounts.create_user(%{
               "username" => "member",
               "display_name" => "Member",
               "password" => "member-long-password",
               "role" => "member"
             })

    assert {:ok, _user} = Accounts.disable_user(user)

    refute Accounts.get_user_by_username_and_password("MEMBER", "member-long-password")
  end

  test "reissued session tokens preserve the original password authentication time" do
    assert {:ok, user} = Accounts.create_admin(@valid)

    authenticated_at =
      DateTime.utc_now() |> DateTime.add(-30, :minute) |> DateTime.truncate(:second)

    token = Accounts.generate_user_session_token(%{user | authenticated_at: authenticated_at})

    assert {fetched_user, _inserted_at} = Accounts.get_user_by_session_token(token)
    assert fetched_user.authenticated_at == authenticated_at
  end
end
