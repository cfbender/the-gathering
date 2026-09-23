defmodule TheGathering.Discord.StartTime do
  @moduledoc "Explicit, timezone-aware syntax for /newgame; nil means start when filled."

  @error "Use 8pm, 20:30, in 45m, tomorrow 7pm, or a future Discord <t:unix> timestamp."

  def parse(input, now \\ DateTime.utc_now(), zone \\ default_timezone())
  def parse(nil, _now, _zone), do: {:ok, nil}

  def parse(input, now, zone) do
    input = input |> String.trim() |> String.downcase()

    with {:ok, datetime} <- parse_input(input, now, zone),
         :gt <- DateTime.compare(datetime, now) do
      {:ok, DateTime.truncate(datetime, :second)}
    else
      {:error, message} when is_binary(message) ->
        {:error, message}

      comparison when comparison in [:lt, :eq] ->
        {:error, "The start time must be in the future."}

      _ ->
        {:error, @error}
    end
  end

  defp default_timezone,
    do:
      Application.get_env(:the_gathering, TheGathering.Discord, [])[:default_timezone] ||
        "America/New_York"

  defp parse_input(input, now, zone) do
    cond do
      match = Regex.run(~r/^<t:(\d{1,11})(?::[tTdDfFRr])?>$/i, input) ->
        DateTime.from_unix(String.to_integer(Enum.at(match, 1)))

      match = Regex.run(~r/^in (\d{1,6})\s*(m|h)$/, input) ->
        [_, amount, unit] = match
        {:ok, DateTime.add(now, String.to_integer(amount) * if(unit == "h", do: 3600, else: 60))}

      true ->
        clock(input, now, zone)
    end
  end

  defp clock(input, now, zone) do
    tomorrow? = String.starts_with?(input, "tomorrow ")
    clock = String.replace_prefix(input, "tomorrow ", "")

    with {:ok, local} <- DateTime.shift_zone(now, zone, Tz.TimeZoneDatabase),
         {:ok, time} <- time(clock) do
      date = Date.add(DateTime.to_date(local), if(tomorrow?, do: 1, else: 0))

      # Compare wall times before conversion: adding 24 UTC hours is wrong across DST.
      date =
        if not tomorrow? and Time.compare(time, DateTime.to_time(local)) != :gt,
          do: Date.add(date, 1),
          else: date

      case DateTime.new(date, time, zone, Tz.TimeZoneDatabase) do
        {:ok, datetime} ->
          DateTime.shift_zone(datetime, "Etc/UTC", Tz.TimeZoneDatabase)

        {:ambiguous, _, _} ->
          {:error, "That clock time occurs twice due to DST. Use a Discord timestamp."}

        {:gap, _, _} ->
          {:error, "That clock time does not exist due to DST. Use a Discord timestamp."}

        error ->
          error
      end
    end
  end

  defp time(input) do
    case Regex.run(~r/^(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$/, input) do
      [_, hour, minute, period] when period in ["am", "pm"] ->
        hour = String.to_integer(hour)
        offset = if period == "pm", do: 12, else: 0

        if hour in 1..12,
          do: Time.new(rem(hour, 12) + offset, minute(minute), 0),
          else: {:error, :invalid_time}

      [_, hour, minute] ->
        Time.new(String.to_integer(hour), minute(minute), 0)

      _ ->
        {:error, :invalid_time}
    end
  end

  defp minute(""), do: 0
  defp minute(value), do: String.to_integer(value)
end
