defmodule TheGathering.Accounts.UserToken do
  use Ecto.Schema
  import Ecto.Query

  alias TheGathering.Accounts.UserToken

  @rand_size 32
  @session_validity_in_days 14

  schema "users_tokens" do
    field :token, :binary
    field :context, :string
    field :sent_to, :string
    field :authenticated_at, :utc_datetime
    belongs_to :user, TheGathering.Accounts.User

    timestamps(type: :utc_datetime, updated_at: false)
  end

  @doc "Builds a random token for a database-tracked cookie session."
  def build_session_token(user) do
    token = :crypto.strong_rand_bytes(@rand_size)
    authenticated_at = user.authenticated_at || DateTime.utc_now() |> DateTime.truncate(:second)

    {token,
     %UserToken{
       token: token,
       context: "session",
       user_id: user.id,
       authenticated_at: authenticated_at
     }}
  end

  @doc "Returns the enabled user and token creation time when a session token is valid."
  def verify_session_token_query(token) do
    query =
      from token in by_token_and_context_query(token, "session"),
        join: user in assoc(token, :user),
        where: token.inserted_at > ago(@session_validity_in_days, "day"),
        where: is_nil(user.disabled_at),
        select: {%{user | authenticated_at: token.authenticated_at}, token.inserted_at}

    {:ok, query}
  end

  defp by_token_and_context_query(token, context) do
    from UserToken, where: [token: ^token, context: ^context]
  end
end
