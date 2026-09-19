defmodule TheGathering.Accounts.ServerSettings do
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :integer, autogenerate: false}
  schema "server_settings" do
    field :registration_enabled, :boolean, default: false

    timestamps(type: :utc_datetime)
  end

  def changeset(settings, attrs) do
    settings
    |> cast(attrs, [:registration_enabled])
    |> validate_required([:registration_enabled])
  end
end
