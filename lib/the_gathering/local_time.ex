defmodule TheGathering.LocalTime do
  @moduledoc """
  Converts between stored UTC timestamps and a viewer's IANA time zone, so date,
  weekday, and hour filters match the local calendar the browser shows.
  """

  @database Tz.TimeZoneDatabase
  @utc "Etc/UTC"

  @doc "Returns `zone` when it names a known IANA time zone, otherwise UTC."
  def zone(zone) when is_binary(zone) and zone != "" do
    case DateTime.now(zone, @database) do
      {:ok, _now} -> zone
      _error -> @utc
    end
  end

  def zone(_zone), do: @utc

  @doc """
  The UTC instant at which `date` begins in `zone`. When DST skips local midnight the
  day starts at the first valid moment after the gap.
  """
  def start_of_day(%Date{} = date, zone) do
    case DateTime.new(date, ~T[00:00:00], zone, @database) do
      {:ok, datetime} -> to_utc(datetime)
      {:ambiguous, first, _second} -> to_utc(first)
      {:gap, _before, just_after} -> to_utc(just_after)
    end
  end

  @doc "Shifts a UTC timestamp into `zone`."
  def to_local(%DateTime{} = datetime, zone), do: DateTime.shift_zone!(datetime, zone, @database)

  @doc "Sunday-first weekday (0 = Sunday … 6 = Saturday), matching JavaScript's `Date#getDay`."
  def weekday(%DateTime{} = datetime), do: datetime |> Date.day_of_week() |> rem(7)

  defp to_utc(datetime), do: DateTime.shift_zone!(datetime, @utc, @database)
end
