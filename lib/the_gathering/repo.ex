defmodule TheGathering.Repo do
  use Ecto.Repo,
    otp_app: :the_gathering,
    adapter: Ecto.Adapters.SQLite3
end
