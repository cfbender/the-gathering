defmodule TheGathering.AccountsTest do
  use TheGathering.DataCase, async: false

  import Ecto.Query

  alias TheGathering.Accounts
  alias TheGathering.Accounts.UserToken
  alias TheGathering.AccountsFixtures
  alias TheGathering.Games
  alias TheGathering.Repo

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

  test "disabling revokes sessions permanently while re-enabled admins can sign in again" do
    assert {:ok, _admin} = Accounts.create_admin(@valid)

    assert {:ok, user} =
             Accounts.create_user(%{
               "username" => "second-admin",
               "display_name" => "Second Admin",
               "password" => "second-admin-password",
               "role" => "admin"
             })

    stolen_token = Accounts.generate_user_session_token(user)
    assert Accounts.get_user_by_session_token(stolen_token)

    assert {:ok, disabled_user} = Accounts.disable_user(user)
    refute Accounts.get_user_by_session_token(stolen_token)

    assert {:ok, _enabled_user} = Accounts.update_user(disabled_user, %{disabled_at: nil})
    refute Accounts.get_user_by_session_token(stolen_token)

    assert fresh_user =
             Accounts.get_user_by_username_and_password("second-admin", "second-admin-password")

    fresh_token = Accounts.generate_user_session_token(fresh_user)
    assert Accounts.get_user_by_session_token(fresh_token)
  end

  describe "display name changes" do
    test "a profile edit renames the linked player" do
      user = AccountsFixtures.user_fixture(%{"display_name" => "cody_discord"})
      {:ok, player} = Games.create_player(%{name: "cody_discord"}, user.id)

      assert {:ok, updated} = Accounts.update_profile(user, %{"display_name" => "  Cody  "})
      assert updated.display_name == "Cody"
      assert Repo.reload!(player).name == "Cody"
    end

    test "an admin edit renames the linked player" do
      user = AccountsFixtures.user_fixture(%{"display_name" => "member"})
      {:ok, player} = Games.create_player(%{name: "member"}, user.id)

      assert {:ok, _updated} = Accounts.update_user(user, %{"display_name" => "Member Name"})
      assert Repo.reload!(player).name == "Member Name"
    end

    test "a name held by another player is rejected without saving either record" do
      user = AccountsFixtures.user_fixture(%{"display_name" => "member"})
      {:ok, player} = Games.create_player(%{name: "member"}, user.id)
      {:ok, _other} = Games.create_player(%{name: "Taken"})

      assert {:error, changeset} = Accounts.update_profile(user, %{"display_name" => "taken"})
      assert "is already used by another player" in errors_on(changeset).display_name
      assert Repo.reload!(user).display_name == "member"
      assert Repo.reload!(player).name == "member"
    end

    test "users without a linked player can still change their display name" do
      user = AccountsFixtures.user_fixture()

      assert {:ok, %{display_name: "Solo"}} =
               Accounts.update_profile(user, %{"display_name" => "Solo"})
    end
  end

  test "revoking all sessions deletes only the target user's tokens" do
    target = AccountsFixtures.user_fixture()
    other_user = AccountsFixtures.user_fixture()

    target_tokens = [
      Accounts.generate_user_session_token(target),
      Accounts.generate_user_session_token(target)
    ]

    other_token = Accounts.generate_user_session_token(other_user)

    assert {:ok, ^target} = Accounts.revoke_all_sessions(target)
    Enum.each(target_tokens, &refute(Accounts.get_user_by_session_token(&1)))
    assert Accounts.get_user_by_session_token(other_token)
  end

  test "issuing a session prunes expired token rows" do
    assert {:ok, user} = Accounts.create_admin(@valid)
    expired_token = Accounts.generate_user_session_token(user)

    Repo.update_all(
      from(token in UserToken, where: token.token == ^expired_token),
      set: [
        inserted_at: DateTime.utc_now() |> DateTime.add(-15, :day) |> DateTime.truncate(:second)
      ]
    )

    fresh_token = Accounts.generate_user_session_token(user)

    refute Repo.exists?(from token in UserToken, where: token.token == ^expired_token)
    assert Accounts.get_user_by_session_token(fresh_token)
  end

  test "Discord sign-in surfaces a player ownership conflict and rolls back the user" do
    {:ok, _admin} = Accounts.create_admin(@valid)
    {:ok, _settings} = Accounts.update_settings(%{"registration_enabled" => true})

    {:ok, owner} =
      Accounts.create_user(%{
        "username" => "player-owner",
        "display_name" => "Player Owner",
        "password" => "another-long-password",
        "role" => "member"
      })

    {:ok, _player} =
      Games.create_player(%{name: "Claimed", discord_id: "discord-conflict"}, owner.id)

    assert {:error, :discord_identity_conflict} =
             Accounts.sign_in_with_discord(%{
               "sub" => "discord-conflict",
               "preferred_username" => "Claimed"
             })

    assert Accounts.get_user_by_discord_id("discord-conflict") == nil
  end

  test "Discord sign-in derives a valid username from handles the format rule rejects" do
    {:ok, _admin} = Accounts.create_admin(@valid)
    {:ok, _settings} = Accounts.update_settings(%{"registration_enabled" => true})

    # Discord allows a leading dot; our usernames must start with a letter or digit.
    assert {:ok, dotted} =
             Accounts.sign_in_with_discord(%{
               "sub" => "dot-1",
               "preferred_username" => ".dreamlan"
             })

    assert dotted.username == "dreamlan"
    assert dotted.display_name == ".dreamlan"

    # A handle that is only punctuation falls back to the generic base.
    assert {:ok, punct} =
             Accounts.sign_in_with_discord(%{"sub" => "dot-2", "preferred_username" => "._."})

    assert punct.username == "discord"

    # Truncation to 32 characters must not leave trailing punctuation behind.
    long = String.duplicate("a", 31) <> "._long_tail"

    assert {:ok, truncated} =
             Accounts.sign_in_with_discord(%{"sub" => "dot-3", "preferred_username" => long})

    assert truncated.username == String.duplicate("a", 31)
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
