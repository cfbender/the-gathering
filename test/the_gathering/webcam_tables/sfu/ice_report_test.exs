defmodule TheGathering.WebcamTables.Sfu.IceReportTest do
  use ExUnit.Case, async: true

  alias TheGathering.WebcamTables.Sfu.IceReport

  @now 100_000

  defp pair(id, local, remote, overrides) do
    Map.merge(
      %{
        id: id,
        type: :candidate_pair,
        local_candidate_id: local,
        remote_candidate_id: remote,
        priority: 1,
        state: :succeeded,
        valid: true,
        nominated: false,
        last_seen: @now - 1_000,
        requests_sent: 0,
        requests_received: 0,
        responses_received: 0,
        non_symmetric_responses_received: 0
      },
      overrides
    )
  end

  # ex_ice reports local candidates under fresh ids (p2 cannot name its local side).
  test "lists the nominated pair first with how long ago the browser was last heard" do
    stats = %{
      :transport => %{
        type: :transport,
        ice_role: :controlled,
        ice_state: :failed,
        dtls_state: :connected,
        packets_received: 1_820,
        packets_sent: 0,
        selected_candidate_pair_changes: 2,
        unmatched_requests: 0
      },
      "l1" => %{
        id: "l1",
        type: :local_candidate,
        candidate_type: :host,
        address: {10, 0, 0, 5},
        port: 50_000
      },
      "r1" => %{
        id: "r1",
        type: :remote_candidate,
        candidate_type: :srflx,
        address: {203, 0, 113, 9},
        port: 61_000
      },
      "r2" => %{
        id: "r2",
        type: :remote_candidate,
        candidate_type: :prflx,
        address: {203, 0, 113, 9},
        port: 61_777
      },
      "p1" =>
        pair("p1", "l1", "r1", %{
          priority: 5,
          state: :failed,
          nominated: true,
          last_seen: @now - 9_200,
          requests_received: 1,
          requests_sent: 4,
          responses_received: 2,
          non_symmetric_responses_received: 2
        }),
      "p2" =>
        pair("p2", "l-unknown", "r2", %{
          priority: 9,
          state: :frozen,
          valid: false,
          last_seen: @now - 40
        })
    }

    assert IceReport.format(stats, @now) ==
             "controlled failed, dtls connected, rx 1820pkt tx 0pkt, selected pair changes 2, " <>
               "unmatched requests 0; local host 10.0.0.5:50000; " <>
               "host 10.0.0.5:50000->srflx 203.0.113.9:61000 failed,nominated,valid seen 9200ms ago " <>
               "req in 1 out 4 resp 2 non-symmetric 2 | " <>
               "prflx 203.0.113.9:61777 frozen seen 40ms ago " <>
               "req in 0 out 0 resp 0"
  end

  test "copes with an mDNS remote address, an unseen pair, and no pairs at all" do
    stats = %{
      :transport => %{
        type: :transport,
        ice_role: :controlled,
        ice_state: :checking,
        dtls_state: :new
      },
      "l1" => %{
        id: "l1",
        type: :local_candidate,
        candidate_type: :host,
        address: {10, 0, 0, 5},
        port: 50_000
      },
      "r1" => %{
        id: "r1",
        type: :remote_candidate,
        candidate_type: :host,
        address: "abc.local",
        port: 9
      },
      "p1" => pair("p1", "l1", "r1", %{state: :waiting, valid: false, last_seen: nil})
    }

    assert IceReport.format(stats, @now) =~
             "host 10.0.0.5:50000->host abc.local:9 waiting seen never"

    assert IceReport.format(%{}, @now) ==
             "unknown unknown, dtls unknown, rx 0pkt tx 0pkt, selected pair changes 0, " <>
               "unmatched requests 0; local none; no candidate pairs"
  end
end
