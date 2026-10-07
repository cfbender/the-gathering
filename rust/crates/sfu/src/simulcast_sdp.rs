//! Keeps a browser's simulcast alive across the server's offers
//! (`TheGathering.WebcamTables.Sfu.SimulcastSdp`).
//!
//! The browser's first offer declares its camera's layers (`a=rid:… send` and
//! `a=simulcast:send …`), and the server reverses them in its answer. A browser that is later
//! offered its camera's media section without them stops sending every layer but the first.
//! `ex_webrtc` dropped the lines from its re-offers; str0m keeps them, so for this server
//! [`restore`] only fills in lines that are actually missing and is otherwise a no-op. It is
//! kept as a guard because losing the lines silently degrades every board to its lowest layer.

use std::collections::BTreeMap;

use crate::browser_sdp::{attribute, split_sections};

/// Simulcast attribute lines (without `a=`) for the server's side of a media section, keyed
/// by its mid.
pub type AttrsByMid = BTreeMap<String, Vec<String>>;

/// The `recv` counterpart of each simulcast-sending media section in a browser offer.
pub fn receiving(sdp: &str) -> AttrsByMid {
    let sections = split_sections(sdp);
    sections
        .media
        .iter()
        .filter_map(|section| {
            let mid = attribute(section, "mid")?;
            let attrs = reverse_simulcast(section);
            (!attrs.is_empty()).then(|| (mid.to_owned(), attrs))
        })
        .collect()
}

/// The server offer with `attrs` added to each media section whose mid they are for, unless
/// the section already declares them.
pub fn restore(sdp: &str, attrs: &AttrsByMid) -> String {
    if attrs.is_empty() {
        return sdp.to_owned();
    }
    let sections = split_sections(sdp);
    let mut restored = String::with_capacity(sdp.len());
    restored.push_str(sections.session);
    for section in &sections.media {
        restored.push_str(section);
        let Some(wanted) = attribute(section, "mid").and_then(|mid| attrs.get(mid)) else {
            continue;
        };
        if !(section.ends_with('\n') || section.is_empty()) {
            restored.push_str("\r\n");
        }
        for attr in wanted.iter().filter(|attr| !declares(section, attr)) {
            restored.push_str("a=");
            restored.push_str(attr);
            restored.push_str("\r\n");
        }
    }
    restored
}

/// Whether `section` already has an attribute for the same rid, or a simulcast line.
fn declares(section: &str, attr: &str) -> bool {
    let key = match attr.split_once(':') {
        Some(("rid", rest)) => rest.split_whitespace().next().map(|id| ("rid", id)),
        Some(("simulcast", _)) => Some(("simulcast", "")),
        _ => None,
    };
    let Some((name, id)) = key else {
        return false;
    };
    section.lines().any(|line| {
        line.strip_prefix("a=")
            .and_then(|rest| rest.strip_prefix(name))
            .and_then(|rest| rest.strip_prefix(':'))
            .is_some_and(|value| id.is_empty() || value.split_whitespace().next() == Some(id))
    })
}

/// `a=rid:<id> send …` becomes `rid:<id> recv …`; `a=simulcast:send X recv Y` becomes
/// `simulcast:send Y recv X` (as `ex_webrtc`'s `SDPUtils.reverse_simulcast/1`).
fn reverse_simulcast(section: &str) -> Vec<String> {
    section
        .lines()
        .filter_map(|line| {
            let attr = line.strip_prefix("a=")?.trim_end();
            if let Some(rid) = attr.strip_prefix("rid:") {
                let mut parts = rid.splitn(3, ' ');
                let id = parts.next()?;
                if parts.next()? != "send" {
                    return None;
                }
                let rest = parts
                    .next()
                    .map(|rest| format!(" {rest}"))
                    .unwrap_or_default();
                return Some(format!("rid:{id} recv{rest}"));
            }
            let simulcast = attr.strip_prefix("simulcast:")?;
            let (mut send, mut recv) = (None, None);
            let mut tokens = simulcast.split_whitespace();
            while let Some(direction) = tokens.next() {
                let groups = tokens.next()?;
                match direction {
                    "send" => send = Some(groups),
                    "recv" => recv = Some(groups),
                    _ => return None,
                }
            }
            // The reversed line: what the browser sends, the server receives.
            let parts: Vec<String> = [("send", recv), ("recv", send)]
                .into_iter()
                .filter_map(|(direction, groups)| {
                    groups.map(|groups| format!("{direction} {groups}"))
                })
                .collect();
            (!parts.is_empty()).then(|| format!("simulcast:{}", parts.join(" ")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = "v=0\no=- 1 2 IN IP4 127.0.0.1\ns=-\nt=0 0\na=group:BUNDLE 0 1\n";

    fn mline(mid: &str, attrs: &[&str]) -> String {
        let mut text = format!(
            "m=video 9 UDP/TLS/RTP/SAVPF 96\nc=IN IP4 0.0.0.0\na=mid:{mid}\na=rtpmap:96 H264/90000\n"
        );
        for attr in attrs {
            text.push_str(attr);
            text.push('\n');
        }
        text
    }

    fn sdp(parts: &[String]) -> String {
        (SESSION.to_owned() + &parts.concat()).replace('\n', "\r\n")
    }

    fn simulcast_offer() -> String {
        sdp(&[
            mline(
                "0",
                &[
                    "a=sendonly",
                    "a=rid:l send",
                    "a=rid:m send",
                    "a=rid:h send",
                    "a=simulcast:send l;m;h",
                ],
            ),
            mline("1", &["a=recvonly"]),
        ])
    }

    fn section_attrs(section: &str, name: &str) -> Vec<String> {
        section
            .lines()
            .filter_map(|line| line.strip_prefix("a="))
            .filter(|attr| attr.starts_with(&format!("{name}:")))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn receiving_reverses_the_browsers_send_layers_for_the_sections_that_have_them() {
        let received = receiving(&simulcast_offer());
        assert!(!received.contains_key("1"));
        assert_eq!(
            received.get("0").unwrap(),
            &[
                "rid:l recv",
                "rid:m recv",
                "rid:h recv",
                "simulcast:recv l;m;h"
            ]
        );
    }

    #[test]
    fn receiving_is_empty_for_a_spectator_offer_and_for_garbage() {
        assert!(receiving(&sdp(&[mline("0", &["a=recvonly"])])).is_empty());
        assert!(receiving("not sdp").is_empty());
    }

    #[test]
    fn restore_adds_the_layers_to_the_matching_section_only() {
        let attrs = receiving(&simulcast_offer());

        let server_offer = sdp(&[
            mline("0", &["a=recvonly"]),
            mline("1", &["a=sendonly", "a=msid:owner-a track-a"]),
            mline("2", &["a=sendonly", "a=msid:owner-b track-b"]),
        ]);

        let restored = restore(&server_offer, &attrs);
        let sections = split_sections(&restored);
        let [camera, board_a, board_b] = sections.media.as_slice() else {
            panic!("three media sections expected");
        };

        assert_eq!(section_attrs(camera, "simulcast"), ["simulcast:recv l;m;h"]);
        assert_eq!(
            section_attrs(camera, "rid"),
            ["rid:l recv", "rid:m recv", "rid:h recv"]
        );
        for board in [board_a, board_b] {
            assert_eq!(section_attrs(board, "simulcast"), Vec::<String>::new());
            assert_eq!(section_attrs(board, "rid"), Vec::<String>::new());
        }
        assert!(
            restored
                .split_inclusive('\n')
                .all(|line| line.ends_with("\r\n"))
        );
    }

    #[test]
    fn restore_leaves_the_offer_alone_when_there_is_nothing_to_restore() {
        let offer = sdp(&[mline("0", &["a=recvonly"])]);
        assert_eq!(restore(&offer, &AttrsByMid::new()), offer);
    }

    #[test]
    fn restore_does_not_repeat_lines_the_offer_already_has() {
        let attrs = receiving(&simulcast_offer());
        let offer = sdp(&[mline(
            "0",
            &[
                "a=recvonly",
                "a=rid:l recv",
                "a=rid:m recv",
                "a=rid:h recv",
                "a=simulcast:recv l;m;h",
            ],
        )]);
        assert_eq!(restore(&offer, &attrs), offer);
    }
}
