defmodule TheGathering.WebcamTables.Sfu.IceReport do
  @moduledoc """
  One log line describing a peer connection's ICE state: the transport summary and every
  candidate pair with how long ago the browser was last heard on it. ex_ice only logs this
  at debug level, which the app compiles out, so this is what tells a NAT rebinding apart
  from a browser that stopped answering.
  """

  alias ExWebRTC.PeerConnection

  @doc "Formats `pc`'s current ICE stats; never raises (the connection may be gone)."
  @spec describe(pid()) :: String.t()
  def describe(pc) do
    stats = PeerConnection.get_stats(pc)
    format(stats, System.monotonic_time(:millisecond))
  catch
    :exit, _reason -> "stats unavailable"
  end

  @doc """
  Formats the ICE part of `PeerConnection.get_stats/1`. `now` is the monotonic millisecond
  clock the pairs' `last_seen` values come from.
  """
  @spec format(map(), integer()) :: String.t()
  def format(stats, now) do
    transport = Map.get(stats, :transport, %{})
    entries = Map.values(stats)
    candidates = Map.new(entries, &{&1[:id], &1})

    # ex_ice reports local candidates under fresh ids, so pairs cannot name their local side;
    # the server has one socket per connection anyway, so they are listed once.
    local =
      entries
      |> Enum.filter(&(&1[:type] == :local_candidate))
      |> Enum.map_join(", ", &candidate/1)

    pairs =
      entries
      |> Enum.filter(&(&1[:type] == :candidate_pair))
      |> Enum.sort_by(&{not &1.nominated, -(&1.priority || 0)})
      |> Enum.map_join(" | ", &pair(&1, candidates, now))

    summary = "#{summary(transport)}; local #{if local == "", do: "none", else: local}"

    if pairs == "", do: "#{summary}; no candidate pairs", else: "#{summary}; #{pairs}"
  end

  # Data counters exist only for the whole connection: ex_ice keeps per-pair ones but
  # leaves them out of the pairs it reports.
  defp summary(transport) do
    get = fn key, default -> Map.get(transport, key) || default end

    "#{get.(:ice_role, :unknown)} #{get.(:ice_state, :unknown)}, " <>
      "dtls #{get.(:dtls_state, :unknown)}, " <>
      "rx #{get.(:packets_received, 0)}pkt tx #{get.(:packets_sent, 0)}pkt, " <>
      "selected pair changes #{get.(:selected_candidate_pair_changes, 0)}, " <>
      "unmatched requests #{get.(:unmatched_requests, 0)}"
  end

  defp pair(pair, candidates, now) do
    flags =
      [
        pair.state,
        if(pair.nominated, do: "nominated"),
        if(pair.valid, do: "valid")
      ]
      |> Enum.reject(&is_nil/1)
      |> Enum.join(",")

    local =
      case candidates[pair.local_candidate_id] do
        nil -> ""
        cand -> candidate(cand) <> "->"
      end

    "#{local}#{candidate(candidates[pair.remote_candidate_id])} #{flags} " <>
      "seen #{age(pair.last_seen, now)} " <>
      "req in #{pair.requests_received} out #{pair.requests_sent} resp #{pair.responses_received}" <>
      non_symmetric(pair.non_symmetric_responses_received)
  end

  defp candidate(nil), do: "?"

  defp candidate(%{candidate_type: type, address: address, port: port}),
    do: "#{type} #{format_address(address)}:#{port}"

  defp format_address(address) when is_tuple(address), do: :inet.ntoa(address) |> to_string()
  defp format_address(address), do: to_string(address)

  defp age(nil, _now), do: "never"
  defp age(last_seen, now), do: "#{max(now - last_seen, 0)}ms ago"

  defp non_symmetric(0), do: ""
  defp non_symmetric(count), do: " non-symmetric #{count}"
end
