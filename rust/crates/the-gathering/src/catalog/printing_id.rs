//! Scryfall printing ids with an optional `-1` suffix for the second face (side or half)
//! of a printing.

use std::sync::LazyLock;

use crate::regex::{Regex, compile};

static ID: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"\A([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})(-1)?\z")
});

/// `parse/1`: the Scryfall card id and the face index (0 or 1), or `None` for anything else.
pub fn parse(id: &str) -> Option<(&str, usize)> {
    let captures = ID.captures(id)?;
    let card_id = captures.get(1)?.as_str();
    let face = usize::from(captures.get(2).is_some());
    Some((card_id, face))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_halves_use_the_same_face_identity_as_reverse_sides() {
        let id = "c2e085dd-a448-4f5a-9cfa-5c2034234e7c";
        assert_eq!(parse(id), Some((id, 0)));
        assert_eq!(parse(&format!("{id}-1")), Some((id, 1)));
        for suffix in ["-0", "-2", "-01", "-1-1", "/1"] {
            assert_eq!(parse(&format!("{id}{suffix}")), None, "{suffix}");
        }
    }
}
