defmodule TheGatheringWeb.ChannelRateLimit do
  @moduledoc """
  Rate limiting for webcam table channels.

  Events are limited per channel connection with token buckets held in the
  socket assigns: `:capacity` is the burst a client may send at once and
  `:refill_per_second` the sustained rate. A bucket lives and dies with its
  connection, so there is nothing to clean up.

  Joins are limited per user through `TheGathering.RateLimiter`, so rejoining
  cannot hand a client a fresh bucket.

  Limits come from `config :the_gathering, TheGatheringWeb.RateLimit`:

      webcam_table_events: [capacity: 60, refill_per_second: 20],
      webcam_table_signals: [capacity: 300, refill_per_second: 50],
      webcam_table_joins: [limit: 30, scale: :timer.minutes(1)]
  """

  alias TheGathering.RateLimiter

  @type bucket :: %{
          capacity: pos_integer(),
          refill_per_ms: number(),
          tokens: number(),
          at: integer()
        }

  @doc "A full bucket configured by `name` (for example `:webcam_table_events`)."
  @spec new(atom(), integer()) :: bucket()
  def new(name, now \\ now()) do
    opts = Keyword.fetch!(config(), name)
    capacity = Keyword.fetch!(opts, :capacity)

    %{
      capacity: capacity,
      refill_per_ms: Keyword.fetch!(opts, :refill_per_second) / 1000,
      tokens: capacity,
      at: now
    }
  end

  @doc "Spends one token, refilling for the time elapsed since the last spend."
  @spec take(bucket(), integer()) :: {:ok, bucket()} | {:error, :rate_limited}
  def take(bucket, now \\ now()) do
    tokens = min(bucket.capacity, bucket.tokens + (now - bucket.at) * bucket.refill_per_ms)

    if tokens >= 1,
      do: {:ok, %{bucket | tokens: tokens - 1, at: now}},
      else: {:error, :rate_limited}
  end

  @doc "Counts a channel join against the user's join budget."
  @spec join(integer()) :: :ok | {:error, :rate_limited}
  def join(user_id) do
    opts = Keyword.fetch!(config(), :webcam_table_joins)
    limit = Keyword.fetch!(opts, :limit)
    scale = Keyword.fetch!(opts, :scale)

    case RateLimiter.hit({:webcam_table_joins, user_id}, scale, limit) do
      {:allow, _count} -> :ok
      {:deny, _retry_after_ms} -> {:error, :rate_limited}
    end
  end

  defp now, do: System.monotonic_time(:millisecond)

  defp config, do: Application.fetch_env!(:the_gathering, TheGatheringWeb.RateLimit)
end
