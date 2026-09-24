defmodule TheGatheringWeb.ChannelRateLimitTest do
  use ExUnit.Case, async: true

  alias TheGatheringWeb.ChannelRateLimit

  # 20 events per second with a burst of 60, the production event bucket.
  @bucket %{capacity: 60, refill_per_ms: 20 / 1000, tokens: 60, at: 0}

  test "allows a burst up to capacity, then refuses until tokens refill" do
    bucket =
      Enum.reduce(1..60, @bucket, fn _, bucket ->
        assert {:ok, bucket} = ChannelRateLimit.take(bucket, 0)
        bucket
      end)

    assert {:error, :rate_limited} = ChannelRateLimit.take(bucket, 0)
    assert {:error, :rate_limited} = ChannelRateLimit.take(bucket, 49)
    assert {:ok, bucket} = ChannelRateLimit.take(bucket, 50)
    assert {:error, :rate_limited} = ChannelRateLimit.take(bucket, 50)
  end

  test "sustained rapid life tapping never runs dry" do
    # Ten minutes of tapping at 15 per second.
    final =
      Enum.reduce(1..9_000, @bucket, fn tap, bucket ->
        assert {:ok, bucket} = ChannelRateLimit.take(bucket, div(tap * 1000, 15))
        bucket
      end)

    assert final.tokens > 50
  end

  test "idle time never refills beyond capacity" do
    assert {:ok, bucket} = ChannelRateLimit.take(%{@bucket | tokens: 0}, :timer.hours(1))
    assert bucket.tokens == 59
  end
end
