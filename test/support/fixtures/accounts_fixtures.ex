defmodule TheGathering.AccountsFixtures do
  @moduledoc "Test helpers for creating `TheGathering.Accounts` entities."

  alias TheGathering.Accounts

  def unique_username, do: "user#{System.unique_integer([:positive])}"
  def valid_user_password, do: "long-enough-password"

  def valid_user_attributes(attrs \\ %{}) do
    Enum.into(attrs, %{
      "username" => unique_username(),
      "display_name" => "Test User",
      "password" => valid_user_password(),
      "role" => "member"
    })
  end

  def user_fixture(attrs \\ %{}) do
    {:ok, user} = attrs |> valid_user_attributes() |> Accounts.create_user()
    user
  end

  def admin_fixture(attrs \\ %{}),
    do: attrs |> Map.new() |> Map.put("role", "admin") |> user_fixture()
end
