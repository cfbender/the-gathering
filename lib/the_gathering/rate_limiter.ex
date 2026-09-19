defmodule TheGathering.RateLimiter do
  @moduledoc """
  In-memory (ETS) rate limiter backing `TheGatheringWeb.RateLimit`.

  Counters live in this node only, which is all a single-container deployment
  needs. `hit/3` returns `{:allow, count}` or `{:deny, retry_after_ms}`.
  """

  use Hammer, backend: :ets
end
