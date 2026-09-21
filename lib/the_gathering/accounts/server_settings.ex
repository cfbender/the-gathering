defmodule TheGathering.Accounts.ServerSettings do
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :integer, autogenerate: false}
  schema "server_settings" do
    field :registration_enabled, :boolean, default: false
    field :registration_invite_hash, :binary, redact: true
    # Games before this date count toward win/loss records only (see TheGathering.Stats).
    field :detailed_stats_from, :date

    timestamps(type: :utc_datetime)
  end

  def changeset(settings, attrs) do
    settings
    |> cast(attrs, [:registration_enabled, :detailed_stats_from], empty_values: ["", nil])
    |> validate_required([:registration_enabled])
  end
end
