defmodule TheGathering.DiscordNewGameAPI do
  @moduledoc false
  use Agent
  alias Nostrum.Api.Helpers

  def start_link(owner),
    do:
      Agent.start_link(fn -> %{owner: owner, now: ~U[2026-09-23 18:00:00Z], failures: []} end,
        name: __MODULE__
      )

  def now, do: Agent.get(__MODULE__, & &1.now)
  def set_now(now), do: Agent.update(__MODULE__, &Map.put(&1, :now, now))
  def fail(operations), do: Agent.update(__MODULE__, &Map.put(&1, :failures, operations))

  def create_response(_interaction, response), do: record(:response, response, {:ok})

  def edit_response(_interaction, response),
    do: record(:edit_response, response, {:ok, %{id: 555}})

  # Exercise Nostrum's mention conversion as well as its integer-only snowflake guards.
  def create(channel, payload) when is_integer(channel) do
    payload = Helpers.prepare_allowed_mentions(payload)
    record(:create, {channel, payload}, {:ok, %{id: 999}})
  end

  def edit(channel, message, payload) when is_integer(channel) and is_integer(message) do
    payload = Helpers.prepare_allowed_mentions(payload)
    record(:edit, {channel, message, payload}, {:ok, %{id: message}})
  end

  def get(333) do
    {:ok,
     %Nostrum.Struct.Guild{
       id: 333,
       owner_id: 42,
       roles: %{
         333 => %Nostrum.Struct.Guild.Role{id: 333, permissions: 0},
         444 => %Nostrum.Struct.Guild.Role{id: 444, permissions: 8},
         445 => %Nostrum.Struct.Guild.Role{id: 445, permissions: 32}
       }
     }}
  end

  def get(_), do: {:error, :not_found}

  defp record(operation, data, success) do
    Agent.get_and_update(__MODULE__, fn state ->
      send(state.owner, {operation, data})

      if operation in state.failures,
        do: {{:error, :network}, %{state | failures: List.delete(state.failures, operation)}},
        else: {success, state}
    end)
  end
end
