defmodule TheGathering.WebcamTables.Timer do
  @moduledoc """
  Pure shared game clock. The server stamps every transition, so clients
  cannot forge times. Repeated actions are idempotent: starting again never
  resets and resuming a running clock changes nothing.
  """

  def new, do: %{started_at: nil, paused_at: nil, paused_ms: 0}

  @doc "Milliseconds of play so far, excluding pauses."
  def elapsed(%{started_at: nil}, _now), do: 0

  def elapsed(timer, now),
    do: max(0, (timer.paused_at || now) - timer.started_at - timer.paused_ms)

  def update(%{started_at: nil} = timer, "start", now), do: %{timer | started_at: now}
  def update(%{started_at: nil} = timer, _action, _now), do: timer
  def update(%{paused_at: nil} = timer, "pause", now), do: %{timer | paused_at: now}

  def update(%{paused_at: paused} = timer, "resume", now) when not is_nil(paused),
    do: %{timer | paused_at: nil, paused_ms: timer.paused_ms + now - paused}

  def update(timer, _action, _now), do: timer
end
