//! One log line describing a peer connection's ICE state
//! (`TheGathering.WebcamTables.Sfu.IceReport`): the transport summary and every candidate
//! pair with how long ago the browser was last heard on it. This is what tells a NAT
//! rebinding apart from a browser that stopped answering.
//!
//! str0m keeps no per-pair statistics of its own, so the room assembles these from what it
//! sees on its sockets: every remote address a peer's datagrams came from is a pair with the
//! local socket, "nominated" when it is the address str0m currently sends to. Connectivity
//! check counters are therefore zero unless the caller knows better.

use std::fmt::Write as _;
use std::net::IpAddr;

/// The connection-wide part of the report. Missing values print as `unknown` or `0`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransportStats {
    pub ice_role: Option<String>,
    pub ice_state: Option<String>,
    pub dtls_state: Option<String>,
    pub packets_received: Option<u64>,
    pub packets_sent: Option<u64>,
    pub selected_candidate_pair_changes: Option<u64>,
    pub unmatched_requests: Option<u64>,
}

/// A candidate's address: an IP, or an mDNS name the browser used to hide it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Address {
    Ip(IpAddr),
    Name(String),
}

/// A local or remote candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateStats {
    pub id: String,
    pub candidate_type: String,
    pub address: Address,
    pub port: u16,
}

/// A candidate pair. `last_seen` is on the same millisecond clock as the report's `now`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairStats {
    pub id: String,
    pub local_candidate_id: String,
    pub remote_candidate_id: String,
    pub priority: Option<u64>,
    pub state: String,
    pub valid: bool,
    pub nominated: bool,
    pub last_seen: Option<i64>,
    pub requests_sent: u64,
    pub requests_received: u64,
    pub responses_received: u64,
    pub non_symmetric_responses_received: u64,
}

/// One entry of the statistics, in the order they should be listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Local(CandidateStats),
    Remote(CandidateStats),
    Pair(PairStats),
}

/// Everything [`format`] reports on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IceStats {
    pub transport: Option<TransportStats>,
    pub entries: Vec<Entry>,
}

/// Formats the statistics; `now` is the millisecond clock the pairs' `last_seen` come from.
pub fn format(stats: &IceStats, now: i64) -> String {
    let transport = stats.transport.clone().unwrap_or_default();

    let local: Vec<String> = stats
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Local(candidate) => Some(describe_candidate(Some(candidate))),
            _ => None,
        })
        .collect();

    let mut pairs: Vec<&PairStats> = stats
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Pair(pair) => Some(pair),
            _ => None,
        })
        .collect();
    // Nominated first, then by descending priority; the sort is stable.
    pairs.sort_by_key(|pair| {
        (
            !pair.nominated,
            std::cmp::Reverse(pair.priority.unwrap_or(0)),
        )
    });
    let pairs: Vec<String> = pairs
        .iter()
        .map(|pair| describe_pair(pair, stats, now))
        .collect();

    let local = if local.is_empty() {
        "none".to_owned()
    } else {
        local.join(", ")
    };
    let summary = format!("{}; local {local}", summary(&transport));
    if pairs.is_empty() {
        format!("{summary}; no candidate pairs")
    } else {
        format!("{summary}; {}", pairs.join(" | "))
    }
}

fn summary(transport: &TransportStats) -> String {
    let text = |value: &Option<String>| value.clone().unwrap_or_else(|| "unknown".to_owned());
    format!(
        "{} {}, dtls {}, rx {}pkt tx {}pkt, selected pair changes {}, unmatched requests {}",
        text(&transport.ice_role),
        text(&transport.ice_state),
        text(&transport.dtls_state),
        transport.packets_received.unwrap_or(0),
        transport.packets_sent.unwrap_or(0),
        transport.selected_candidate_pair_changes.unwrap_or(0),
        transport.unmatched_requests.unwrap_or(0),
    )
}

fn find_candidate<'a>(stats: &'a IceStats, id: &str) -> Option<&'a CandidateStats> {
    stats.entries.iter().find_map(|entry| match entry {
        Entry::Local(candidate) | Entry::Remote(candidate) if candidate.id == id => Some(candidate),
        _ => None,
    })
}

fn describe_pair(pair: &PairStats, stats: &IceStats, now: i64) -> String {
    let mut flags = vec![pair.state.clone()];
    if pair.nominated {
        flags.push("nominated".to_owned());
    }
    if pair.valid {
        flags.push("valid".to_owned());
    }
    let local = find_candidate(stats, &pair.local_candidate_id)
        .map(|candidate| format!("{}->", describe_candidate(Some(candidate))))
        .unwrap_or_default();
    let remote = describe_candidate(find_candidate(stats, &pair.remote_candidate_id));
    let age = pair.last_seen.map_or_else(
        || "never".to_owned(),
        |seen| format!("{}ms ago", (now - seen).max(0)),
    );
    let mut line = format!(
        "{local}{remote} {} seen {age} req in {} out {} resp {}",
        flags.join(","),
        pair.requests_received,
        pair.requests_sent,
        pair.responses_received
    );
    if pair.non_symmetric_responses_received > 0 {
        let _ = write!(
            line,
            " non-symmetric {}",
            pair.non_symmetric_responses_received
        );
    }
    line
}

fn describe_candidate(candidate: Option<&CandidateStats>) -> String {
    match candidate {
        None => "?".to_owned(),
        Some(candidate) => {
            let address = match &candidate.address {
                Address::Ip(ip) => ip.to_string(),
                Address::Name(name) => name.clone(),
            };
            format!("{} {address}:{}", candidate.candidate_type, candidate.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 100_000;

    fn pair(id: &str, local: &str, remote: &str) -> PairStats {
        PairStats {
            id: id.to_owned(),
            local_candidate_id: local.to_owned(),
            remote_candidate_id: remote.to_owned(),
            priority: Some(1),
            state: "succeeded".to_owned(),
            valid: true,
            nominated: false,
            last_seen: Some(NOW - 1_000),
            requests_sent: 0,
            requests_received: 0,
            responses_received: 0,
            non_symmetric_responses_received: 0,
        }
    }

    fn candidate(id: &str, kind: &str, address: Address, port: u16) -> CandidateStats {
        CandidateStats {
            id: id.to_owned(),
            candidate_type: kind.to_owned(),
            address,
            port,
        }
    }

    fn ip(text: &str) -> Address {
        Address::Ip(text.parse().unwrap())
    }

    // Local candidates may be reported under ids no pair names (p2 cannot name its local side).
    #[test]
    fn lists_the_nominated_pair_first_with_how_long_ago_the_browser_was_last_heard() {
        let stats = IceStats {
            transport: Some(TransportStats {
                ice_role: Some("controlled".into()),
                ice_state: Some("failed".into()),
                dtls_state: Some("connected".into()),
                packets_received: Some(1_820),
                packets_sent: Some(0),
                selected_candidate_pair_changes: Some(2),
                unmatched_requests: Some(0),
            }),
            entries: vec![
                Entry::Local(candidate("l1", "host", ip("10.0.0.5"), 50_000)),
                Entry::Pair(PairStats {
                    priority: Some(5),
                    state: "failed".into(),
                    nominated: true,
                    last_seen: Some(NOW - 9_200),
                    requests_received: 1,
                    requests_sent: 4,
                    responses_received: 2,
                    non_symmetric_responses_received: 2,
                    ..pair("p1", "l1", "r1")
                }),
                Entry::Pair(PairStats {
                    priority: Some(9),
                    state: "frozen".into(),
                    valid: false,
                    last_seen: Some(NOW - 40),
                    ..pair("p2", "l-unknown", "r2")
                }),
                Entry::Remote(candidate("r1", "srflx", ip("203.0.113.9"), 61_000)),
                Entry::Remote(candidate("r2", "prflx", ip("203.0.113.9"), 61_777)),
            ],
        };

        assert_eq!(
            format(&stats, NOW),
            "controlled failed, dtls connected, rx 1820pkt tx 0pkt, selected pair changes 2, \
             unmatched requests 0; local host 10.0.0.5:50000; \
             host 10.0.0.5:50000->srflx 203.0.113.9:61000 failed,nominated,valid seen 9200ms ago \
             req in 1 out 4 resp 2 non-symmetric 2 | \
             prflx 203.0.113.9:61777 frozen seen 40ms ago \
             req in 0 out 0 resp 0"
        );
    }

    #[test]
    fn copes_with_an_mdns_remote_address_an_unseen_pair_and_no_pairs_at_all() {
        let stats = IceStats {
            transport: Some(TransportStats {
                ice_role: Some("controlled".into()),
                ice_state: Some("checking".into()),
                dtls_state: Some("new".into()),
                ..TransportStats::default()
            }),
            entries: vec![
                Entry::Local(candidate("l1", "host", ip("10.0.0.5"), 50_000)),
                Entry::Remote(candidate(
                    "r1",
                    "host",
                    Address::Name("abc.local".into()),
                    9,
                )),
                Entry::Pair(PairStats {
                    state: "waiting".into(),
                    valid: false,
                    last_seen: None,
                    ..pair("p1", "l1", "r1")
                }),
            ],
        };

        assert!(
            format(&stats, NOW)
                .contains("host 10.0.0.5:50000->host abc.local:9 waiting seen never")
        );

        assert_eq!(
            format(&IceStats::default(), NOW),
            "unknown unknown, dtls unknown, rx 0pkt tx 0pkt, selected pair changes 0, \
             unmatched requests 0; local none; no candidate pairs"
        );
    }
}
