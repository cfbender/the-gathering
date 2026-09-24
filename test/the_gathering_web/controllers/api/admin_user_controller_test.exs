defmodule TheGatheringWeb.API.AdminUserControllerTest do
  use TheGatheringWeb.ConnCase, async: false

  import Ecto.Query

  alias TheGathering.Accounts
  alias TheGathering.Accounts.{User, UserToken}
  alias TheGathering.AccountsFixtures
  alias TheGathering.Games
  alias TheGathering.Games.{Game, Player}
  alias TheGathering.Repo

  test "DELETE refuses a player with games without changing their account or history", %{
    conn: conn
  } do
    admin = AccountsFixtures.admin_fixture()
    {:ok, _settings} = Accounts.update_settings(%{"registration_enabled" => true})
    user = discord_user("100000000000000101")
    player = Repo.get_by!(Player, user_id: user.id)
    {:ok, opponent} = Games.create_player(%{name: "Opponent"})

    {:ok, deck} =
      Games.create_deck(%{
        player_id: player.id,
        name: "History Deck",
        commander_name: "Alela, Artful Provocateur"
      })

    {:ok, game} =
      Games.create_game(
        %{
          played_at: ~U[2026-09-20 18:00:00Z],
          seats: [
            %{player_id: player.id, deck_id: deck.id, seat: 1, result: "win"},
            %{player_id: opponent.id, seat: 2, result: "loss"}
          ]
        },
        user.id
      )

    target_token = Accounts.generate_user_session_token(user)

    conn = conn |> log_in_user(admin) |> delete(~p"/api/admin/users/#{user.id}")

    assert json_response(conn, 422) == %{
             "errors" => %{"player" => ["must have zero games before deleting this user"]}
           }

    assert Repo.get(User, user.id)
    assert Accounts.get_user_by_session_token(target_token)
    assert Repo.exists?(from token in UserToken, where: token.user_id == ^user.id)

    assert Repo.get!(Player, player.id).user_id == user.id
    assert Repo.get!(Player, player.id).discord_id == user.discord_id

    assert Repo.get!(Game, game.id).created_by_user_id == user.id
    assert Games.get_deck!(deck.id).player_id == player.id
    assert Enum.map(Games.get_game!(game.id).seats, & &1.player_id) == [player.id, opponent.id]
  end

  test "DELETE removes a zero-game player, their decks, account and sessions", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    {:ok, _settings} = Accounts.update_settings(%{"registration_enabled" => true})
    user = discord_user("delete-empty-player")
    player = Repo.get_by!(Player, user_id: user.id)

    {:ok, deck} =
      Games.create_deck(%{player_id: player.id, name: "Unused", commander_name: "Alela"})

    {:ok, other} = Games.create_player(%{name: "Unrelated", discord_id: "keep-this-identity"})
    target_token = Accounts.generate_user_session_token(user)

    assert conn |> log_in_user(admin) |> delete(~p"/api/admin/users/#{user.id}") |> response(204)
    refute Accounts.get_user(user.id)
    refute Games.get_player(player.id)
    refute Games.get_deck(deck.id)
    refute Accounts.get_user_by_session_token(target_token)
    assert Games.get_player(other.id).discord_id == "keep-this-identity"
  end

  test "DELETE allows an account without a player and preserves games they recorded", %{
    conn: conn
  } do
    admin = AccountsFixtures.admin_fixture()
    user = AccountsFixtures.user_fixture()
    {:ok, one} = Games.create_player(%{name: "One"})
    {:ok, two} = Games.create_player(%{name: "Two"})

    {:ok, game} =
      Games.create_game(
        %{
          played_at: ~U[2026-09-20 18:00:00Z],
          seats: [
            %{player_id: one.id, seat: 1, result: "win"},
            %{player_id: two.id, seat: 2, result: "loss"}
          ]
        },
        user.id
      )

    assert conn |> log_in_user(admin) |> delete(~p"/api/admin/users/#{user.id}") |> response(204)
    assert Games.get_game(game.id).created_by_user_id == nil
    assert length(Games.get_game!(game.id).seats) == 2
  end

  test "an administrator cannot delete their own account", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()

    response =
      conn
      |> log_in_user(admin)
      |> delete(~p"/api/admin/users/#{admin.id}")
      |> json_response(403)

    assert response == %{"errors" => %{"detail" => "Forbidden"}}
    assert Repo.get(User, admin.id)
  end

  test "DELETE sessions revokes the target user's sessions", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    user = AccountsFixtures.user_fixture()
    target_token = Accounts.generate_user_session_token(user)
    admin_token = Accounts.generate_user_session_token(admin)

    response =
      conn
      |> log_in_user(admin)
      |> delete(~p"/api/admin/users/#{user.id}/sessions")
      |> json_response(200)

    assert response["data"]["id"] == user.id
    assert response["data"]["disabled"] == false
    refute Accounts.get_user_by_session_token(target_token)
    assert Accounts.get_user_by_session_token(admin_token)
  end

  test "DELETE sessions returns 404 for an unknown user", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()

    response =
      conn
      |> log_in_user(admin)
      |> delete(~p"/api/admin/users/0/sessions")
      |> json_response(404)

    assert response == %{"errors" => %{"detail" => "Not Found"}}
  end

  test "session revocation requires recent sudo authentication", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    member = AccountsFixtures.user_fixture()
    conn = log_in_user(conn, admin)
    token = get_session(conn, :user_token)

    Repo.update_all(
      from(user_token in UserToken, where: user_token.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    response = conn |> delete(~p"/api/admin/users/#{member.id}/sessions") |> json_response(403)

    assert response == %{
             "errors" => %{
               "code" => "sudo_required",
               "detail" => "Reauthentication required"
             }
           }
  end

  test "the last administrator cannot be deleted" do
    admin = AccountsFixtures.admin_fixture()
    member = AccountsFixtures.user_fixture()

    assert {:error, :forbidden} = Accounts.delete_user(admin, member)
    assert Repo.get(User, admin.id)
  end

  test "a non-administrator receives 403", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    member = AccountsFixtures.user_fixture()

    response =
      conn
      |> log_in_user(member)
      |> delete(~p"/api/admin/users/#{admin.id}")
      |> json_response(403)

    assert response == %{"errors" => %{"detail" => "Forbidden"}}
  end

  test "deletion requires recent sudo authentication", %{conn: conn} do
    admin = AccountsFixtures.admin_fixture()
    member = AccountsFixtures.user_fixture()
    conn = log_in_user(conn, admin)
    token = get_session(conn, :user_token)

    Repo.update_all(
      from(user_token in UserToken, where: user_token.token == ^token),
      set: [
        authenticated_at:
          DateTime.utc_now() |> DateTime.add(-11, :minute) |> DateTime.truncate(:second)
      ]
    )

    response = conn |> delete(~p"/api/admin/users/#{member.id}") |> json_response(403)

    assert response == %{
             "errors" => %{
               "code" => "sudo_required",
               "detail" => "Reauthentication required"
             }
           }

    assert Repo.get(User, member.id)
  end

  defp discord_user(discord_id) do
    {:ok, user} = Accounts.sign_in_with_discord(discord_claims(discord_id))
    user
  end

  defp discord_claims(discord_id) do
    %{
      "sub" => discord_id,
      "preferred_username" => "Discord Member",
      "picture" => "https://cdn.discordapp.com/avatars/#{discord_id}/avatar-hash"
    }
  end
end
