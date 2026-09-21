defmodule TheGathering.Repo.Migrations.AddRegistrationInviteHash do
  use Ecto.Migration

  def change do
    alter table(:server_settings) do
      add :registration_invite_hash, :binary
    end
  end
end
