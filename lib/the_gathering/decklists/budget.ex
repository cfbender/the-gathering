defmodule TheGathering.Decklists.Budget do
  @moduledoc false

  use Agent

  def start_link(options) do
    now = now_ms()

    Agent.start_link(fn ->
      %{bytes_left: options[:max_bytes], deadline: now + options[:duration_ms]}
    end)
  end

  def consume(budget, bytes) do
    Agent.get_and_update(budget, fn state ->
      if bytes <= state.bytes_left do
        {:ok, %{state | bytes_left: state.bytes_left - bytes}}
      else
        {{:error, :byte_limit}, state}
      end
    end)
  end

  def remaining_ms(budget) do
    Agent.get(budget, &max(&1.deadline - now_ms(), 0))
  end

  def stop(budget), do: Agent.stop(budget)

  defp now_ms do
    clock = Application.get_env(:the_gathering, :decklists_clock, &System.monotonic_time/1)
    clock.(:millisecond)
  end
end
