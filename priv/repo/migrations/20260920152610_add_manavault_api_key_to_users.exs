defmodule TheGathering.Repo.Migrations.AddManavaultApiKeyToUsers do
  use Ecto.Migration

  def change do
    alter table(:users) do
      # Encrypted at rest by TheGathering.Accounts.EncryptedString.
      add :manavault_api_key, :text
    end
  end
end
