//! Smooths over browser SDP that a strict parser rejects but the browser means harmlessly.
//!
//! Firefox and Safari answer the server's re-offers with their original DTLS role
//! (`a=setup:passive`) on the media sections they already had and `a=setup:active` on the ones
//! the offer added. All of those sections share one bundled transport, so the role of the
//! BUNDLE-tagged section is the only one that counts (RFC 8843 §7.1). str0m reads the role of
//! the first section that has one, which is not necessarily the tagged one; copying the tagged
//! section's role onto the others gives it the SDP it should read without changing what the
//! browser will do.

/// The SDP split into its session part and its media sections, each keeping its own line
/// breaks so joining them gives back the input.
pub(crate) struct Sections<'a> {
    pub(crate) session: &'a str,
    pub(crate) media: Vec<&'a str>,
}

/// Splits `sdp` at every line that starts with `m=`.
pub(crate) fn split_sections(sdp: &str) -> Sections<'_> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in sdp.split_inclusive('\n') {
        if line.starts_with("m=") {
            starts.push(offset);
        }
        offset += line.len();
    }
    let first = starts.first().copied().unwrap_or(sdp.len());
    let session = sdp.get(..first).unwrap_or(sdp);
    let media = starts
        .iter()
        .enumerate()
        .filter_map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(sdp.len());
            sdp.get(*start..end)
        })
        .collect();
    Sections { session, media }
}

/// The value of the first `a=<name>:<value>` line in `text` (up to the first whitespace).
pub(crate) fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let rest = line
            .strip_prefix("a=")?
            .strip_prefix(name)?
            .strip_prefix(':')?;
        rest.split_whitespace().next()
    })
}

/// The description with every media section's `a=setup` set to the bundled transport's.
pub fn unify_dtls_roles(sdp: &str) -> String {
    let sections = split_sections(sdp);
    let Some(role) = role_of(&sections) else {
        return sdp.to_owned();
    };
    let mut unified = String::with_capacity(sdp.len());
    unified.push_str(sections.session);
    for section in &sections.media {
        for line in section.split_inclusive('\n') {
            match line.strip_prefix("a=setup:") {
                Some(rest) => {
                    // Keep whatever follows the role token (the line break).
                    let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    unified.push_str("a=setup:");
                    unified.push_str(role);
                    unified.push_str(rest.get(token_end..).unwrap_or(""));
                }
                None => unified.push_str(line),
            }
        }
    }
    unified
}

fn role_of<'a>(sections: &Sections<'a>) -> Option<&'a str> {
    let tagged = bundle_tag(sections.session).and_then(|tag| {
        sections
            .media
            .iter()
            .find(|section| attribute(section, "mid") == Some(tag))
    });
    tagged
        .into_iter()
        .chain(sections.media.iter())
        .find_map(|section| attribute(section, "setup"))
}

/// The first mid of the session's `a=group:BUNDLE` line.
fn bundle_tag(session: &str) -> Option<&str> {
    session.lines().find_map(|line| {
        let rest = line.strip_prefix("a=group:BUNDLE")?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        rest.split_whitespace().next()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sdp(bundle: &str, sections: &[String]) -> String {
        let session = format!(
            "v=0\no=mozilla...THIS_IS_SDPARTA-99.0 1 2 IN IP4 0.0.0.0\ns=-\nt=0 0\na=group:BUNDLE {bundle}\na=fingerprint:sha-256 AA:BB\n"
        );
        (session + &sections.concat()).replace('\n', "\r\n")
    }

    fn section(mid: &str, setup: Option<&str>) -> String {
        let setup = setup
            .map(|role| format!("a=setup:{role}\n"))
            .unwrap_or_default();
        format!(
            "m=video 9 UDP/TLS/RTP/SAVPF 120\nc=IN IP4 0.0.0.0\na=mid:{mid}\n{setup}a=rtpmap:120 VP8/90000\n"
        )
    }

    fn roles(sdp: &str) -> Vec<String> {
        sdp.lines()
            .filter_map(|line| line.strip_prefix("a=setup:"))
            .map(|role| role.trim_end().to_owned())
            .collect()
    }

    fn without_roles(sdp: &str) -> String {
        sdp.split_inclusive('\n')
            .map(|line| {
                if line.starts_with("a=setup:") {
                    "a=setup:\r\n"
                } else {
                    line
                }
            })
            .collect()
    }

    #[test]
    fn copies_the_bundle_tagged_sections_role_onto_the_others() {
        let firefox_answer = sdp(
            "0 1 2",
            &[
                section("0", Some("passive")),
                section("1", Some("active")),
                section("2", Some("active")),
            ],
        );

        let unified = unify_dtls_roles(&firefox_answer);

        assert_eq!(roles(&unified), ["passive", "passive", "passive"]);
        assert_eq!(without_roles(&unified), without_roles(&firefox_answer));
    }

    #[test]
    fn follows_the_tag_rather_than_the_first_section() {
        let answer = sdp(
            "1 0",
            &[section("0", Some("active")), section("1", Some("passive"))],
        );

        assert_eq!(roles(&unify_dtls_roles(&answer)), ["passive", "passive"]);
    }

    #[test]
    fn falls_back_to_the_first_role_when_the_tagged_section_has_none() {
        let answer = sdp("0 1", &[section("0", None), section("1", Some("active"))]);

        assert_eq!(roles(&unify_dtls_roles(&answer)), ["active"]);
    }

    #[test]
    fn leaves_consistent_and_role_less_descriptions_alone() {
        let chrome_answer = sdp(
            "0 1",
            &[section("0", Some("active")), section("1", Some("active"))],
        );
        assert_eq!(unify_dtls_roles(&chrome_answer), chrome_answer);

        let bare = sdp("0", &[section("0", None)]);
        assert_eq!(unify_dtls_roles(&bare), bare);

        assert_eq!(unify_dtls_roles("not sdp"), "not sdp");
    }
}
