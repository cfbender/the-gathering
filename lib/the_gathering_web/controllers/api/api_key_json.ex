defmodule TheGatheringWeb.API.ApiKeyJSON do
  alias TheGathering.Accounts.ApiKey

  def index(%{api_keys: api_keys}), do: %{data: Enum.map(api_keys, &data/1)}

  # The secret is only ever rendered here, in the creation response.
  def create(%{api_key: api_key, token: token}),
    do: %{data: Map.put(data(api_key), :token, token)}

  def data(%ApiKey{} = api_key) do
    %{
      id: api_key.id,
      name: api_key.name,
      prefix: api_key.prefix,
      last_used_at: api_key.last_used_at,
      inserted_at: api_key.inserted_at
    }
  end
end
