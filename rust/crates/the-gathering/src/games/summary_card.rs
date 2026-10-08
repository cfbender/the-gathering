//! The game summary card as SVG.

use std::collections::HashMap;
use std::fmt::Write;

use crate::regex::{Regex, compile};
use std::sync::LazyLock;

use super::model::{Deck, Game, GameResult, GameSource, Seat};
use super::win_condition::WinCondition;

/// A commander slot's `(card id, name, printing id)`, the key of downloaded art.
pub type ArtKey = (Option<String>, String, Option<String>);

/// Downloaded art as `data:` URIs.
pub type Images = HashMap<ArtKey, String>;

static CONTROL: LazyLock<Regex> = LazyLock::new(|| compile(r"[\x00-\x08\x0B\x0C\x0E-\x1F]"));
static SPACE: LazyLock<Regex> = LazyLock::new(|| compile(r"\s+"));

/// HTML-escapes text after dropping control characters.
fn escape(text: &str) -> String {
    let cleaned = CONTROL.replace_all(text, "");
    let mut out = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn length(text: &str) -> usize {
    text.chars().count()
}

fn truncate(text: &str, limit: usize) -> String {
    if length(text) > limit {
        let mut out: String = text.chars().take(limit.saturating_sub(1)).collect();
        out.push('…');
        out
    } else {
        text.to_owned()
    }
}

/// Word-wraps `text` to `count` lines of `width` characters, ellipsizing the last.
fn lines(text: &str, width: usize, count: usize) -> Vec<String> {
    let normalized = SPACE.replace_all(text, " ");
    let mut all: Vec<String> = vec![String::new()];
    for word in normalized.trim().split(' ') {
        let line = all.last().cloned().unwrap_or_default();
        let joined = format!("{line} {word}");
        if length(&joined) <= width {
            if let Some(last) = all.last_mut() {
                joined.trim().clone_into(last);
            }
        } else {
            all.push(truncate(word, width));
        }
    }
    let all: Vec<String> = all.into_iter().filter(|line| !line.is_empty()).collect();
    let mut visible: Vec<String> = all.iter().take(count).cloned().collect();
    if all.len() > count
        && let Some(last) = visible.last_mut()
    {
        *last = format!("{}…", truncate(last, width.saturating_sub(1)));
    }
    visible
}

/// A float as text, always with a decimal point.
fn float(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn commanders(deck: Option<&Deck>) -> String {
    match deck {
        None => "Commander not recorded".to_owned(),
        Some(deck) => {
            let mut names = vec![deck.commander_name.as_str()];
            if let Some(partner) = deck.partner_name.as_deref() {
                names.push(partner);
            }
            names.join(" / ")
        }
    }
}

fn art<'a>(deck: Option<&Deck>, images: &'a Images, partner: bool) -> Option<&'a String> {
    let deck = deck?;
    let key = if partner {
        (
            deck.partner_card_id.clone(),
            deck.partner_name.clone()?,
            deck.partner_printing_id.clone(),
        )
    } else {
        (
            deck.commander_card_id.clone(),
            deck.commander_name.clone(),
            deck.commander_printing_id.clone(),
        )
    };
    images.get(&key)
}

fn usize_f(value: usize) -> f64 {
    u32::try_from(value).map_or(f64::from(u32::MAX), f64::from)
}

fn int(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Alt text for the image (at most 1024 characters).
pub fn description(game: &Game) -> String {
    let result = game.winner().map_or_else(
        || "Draw".to_owned(),
        |winner| format!("Winner: {}", winner.player.name),
    );
    let text = format!(
        "Game #{}. {result}. {}. {} players.",
        game.id,
        WinCondition::label_of(game.win_condition),
        game.seats.len()
    );
    text.chars().take(1024).collect()
}

/// The game's summary card as an SVG document.
pub fn svg(game: &Game, images: &Images) -> String {
    let mut seats: Vec<&Seat> = game.seats.iter().collect();
    seats.sort_by_key(|seat| seat.seat);
    let winner = seats
        .iter()
        .copied()
        .find(|seat| seat.result == GameResult::Win);
    let seat_count = seats.len().max(1);
    let body_height = int(seats.len()).saturating_mul(88).max(380);
    let notes = lines(
        game.notes.as_deref().unwrap_or("No notes recorded."),
        100,
        3,
    );
    let names = lines(
        winner.map_or("A shared finish", |winner| winner.player.name.as_str()),
        23,
        2,
    );
    let commander_text = winner.map_or_else(
        || "No winner at this table".to_owned(),
        |winner| commanders(winner.deck.as_ref()),
    );
    let commander_lines = lines(&commander_text, 43, 2);
    let commander_y = body_height + 72 - int(commander_lines.len().saturating_sub(1)) * 23;
    let name_y = commander_y - 32 - int(names.len().saturating_sub(1)) * 40;
    let height = body_height + 228 + int(notes.len()) * 24;
    let row_height =
        f64::from(i32::try_from(body_height).unwrap_or(i32::MAX)) / usize_f(seat_count);

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="1200" height="{height}" viewBox="0 0 1200 {height}">"#
    );
    out.push_str(
        r##"  <defs>
    <linearGradient id="shade" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#10131c" stop-opacity="0.1"/>
      <stop offset="0.45" stop-color="#10131c" stop-opacity="0.25"/>
      <stop offset="1" stop-color="#10131c" stop-opacity="0.98"/>
    </linearGradient>
"##,
    );
    let _ = writeln!(
        out,
        r#"    <clipPath id="hero"><rect x="32" y="100" width="520" height="{body_height}" rx="18"/></clipPath>"#
    );
    let seat_y = |seat: &Seat| {
        100.0
            + f64::from(i32::try_from(seat.seat - 1).unwrap_or_default()) * row_height
            + (row_height - 88.0) / 2.0
    };
    for seat in &seats {
        let y = seat_y(seat);
        let _ = writeln!(
            out,
            r#"      <clipPath id="portrait-{n}"><circle cx="614" cy="{cy}" r="27"/></clipPath>"#,
            n = seat.seat,
            cy = float(y + 37.0)
        );
        let _ = writeln!(
            out,
            r#"      <clipPath id="partner-{n}"><circle cx="634" cy="{cy}" r="18"/></clipPath>"#,
            n = seat.seat,
            cy = float(y + 51.0)
        );
    }
    out.push_str("  </defs>\n");
    let _ = writeln!(
        out,
        r##"  <rect width="1200" height="{height}" fill="#131620"/>"##
    );
    out.push_str(
        r##"  <g font-family="DejaVu Sans, sans-serif" fill="#f0edf8">
    <text x="32" y="46" font-size="22" font-weight="bold" letter-spacing="3">THE GATHERING</text>
    <text x="32" y="73" font-size="15" fill="#aaa6bc">THE LAST WORD AT THE TABLE</text>
"##,
    );
    let spellbot = if game.source == GameSource::Discord {
        let external = game.external_id.as_deref().unwrap_or_default();
        format!(
            " · {}",
            escape(external.strip_prefix("spellbot:").unwrap_or(external))
        )
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        r#"    <text x="1168" y="44" text-anchor="end" font-size="18" font-weight="bold">GAME #{}{spellbot}</text>"#,
        game.id
    );
    let played = game.played_at.inner();
    let month = played.month().to_string();
    let _ = writeln!(
        out,
        r##"    <text x="1168" y="73" text-anchor="end" font-size="15" fill="#aaa6bc">{} {}, {} · {:02}:{:02} UTC</text>"##,
        month.get(..3).unwrap_or(&month),
        played.day(),
        played.year(),
        played.hour(),
        played.minute()
    );
    out.push('\n');
    let _ = writeln!(
        out,
        r##"    <rect x="32" y="100" width="520" height="{body_height}" rx="18" fill="#29263c"/>"##
    );
    if let Some(hero) = winner.and_then(|winner| art(winner.deck.as_ref(), images, false)) {
        let _ = writeln!(
            out,
            r#"      <image x="32" y="100" width="520" height="{body_height}" preserveAspectRatio="xMidYMid slice" clip-path="url(#hero)" xlink:href="{hero}"/>"#
        );
    }
    let _ = writeln!(
        out,
        r#"    <rect x="32" y="100" width="520" height="{body_height}" rx="18" fill="url(#shade)"/>"#
    );
    out.push_str(r##"    <rect x="52" y="120" width="480" height="40" rx="20" fill="#171923" fill-opacity="0.88"/>"##);
    out.push('\n');
    let _ = writeln!(
        out,
        r##"    <text x="72" y="146" font-size="17" font-weight="bold" fill="#e8c67e">{}</text>"##,
        escape(WinCondition::label_of(game.win_condition))
    );
    let _ = writeln!(
        out,
        r##"    <text x="56" y="{}" font-size="15" font-weight="bold" letter-spacing="3" fill="#e8c67e">{}</text>"##,
        name_y - 42,
        if winner.is_some() { "WINNER" } else { "DRAW" }
    );
    for (index, line) in names.iter().enumerate() {
        let _ = writeln!(
            out,
            r#"      <text x="56" y="{}" font-family="DejaVu Sans Mono" font-size="34" font-weight="bold">{}</text>"#,
            name_y + int(index) * 40,
            escape(line)
        );
    }
    for (index, line) in commander_lines.iter().enumerate() {
        let _ = writeln!(
            out,
            r##"      <text x="56" y="{}" font-family="DejaVu Sans Mono" font-size="17" fill="#d2cddd">{}</text>"##,
            commander_y + int(index) * 23,
            escape(line)
        );
    }
    out.push('\n');
    for seat in &seats {
        let top = 100.0 + f64::from(i32::try_from(seat.seat - 1).unwrap_or_default()) * row_height;
        let y = seat_y(seat);
        let won = seat.result == GameResult::Win;
        let _ = writeln!(
            out,
            r#"      <rect x="572" y="{}" width="596" height="{}" rx="14" fill="{}"/>"#,
            float(top),
            float(row_height - 10.0),
            if won { "#302c28" } else { "#202330" }
        );
        let _ = writeln!(
            out,
            r##"      <circle cx="614" cy="{}" r="28" fill="#49445e"/>"##,
            float(y + 37.0)
        );
        let initial: String = seat.player.name.chars().take(1).collect();
        let _ = writeln!(
            out,
            r#"      <text x="614" y="{}" text-anchor="middle" font-size="20">{}</text>"#,
            float(y + 44.0),
            escape(&initial)
        );
        if let Some(portrait) = art(seat.deck.as_ref(), images, false) {
            let _ = writeln!(
                out,
                r#"        <image x="587" y="{}" width="54" height="54" preserveAspectRatio="xMidYMid slice" clip-path="url(#portrait-{})" xlink:href="{portrait}"/>"#,
                float(y + 10.0),
                seat.seat
            );
        }
        if let Some(partner) = art(seat.deck.as_ref(), images, true) {
            let _ = writeln!(
                out,
                r#"        <image x="616" y="{}" width="36" height="36" preserveAspectRatio="xMidYMid slice" clip-path="url(#partner-{})" xlink:href="{partner}"/>"#,
                float(y + 33.0),
                seat.seat
            );
        }
        let _ = writeln!(
            out,
            r#"      <text x="662" y="{}" font-family="DejaVu Sans Mono" font-size="19" font-weight="bold" fill="{}">{}</text>"#,
            float(y + 26.0),
            if won { "#e8c67e" } else { "#f0edf8" },
            escape(&truncate(&seat.player.name, 31))
        );
        for (index, line) in lines(&commanders(seat.deck.as_ref()), 48, 2)
            .iter()
            .enumerate()
        {
            let _ = writeln!(
                out,
                r##"        <text x="662" y="{}" font-family="DejaVu Sans Mono" font-size="14" fill="#bcb6cb">{}</text>"##,
                float(y + 47.0 + f64::from(i32::try_from(index).unwrap_or_default()) * 18.0),
                escape(line)
            );
        }
        let _ = writeln!(
            out,
            r##"      <text x="1132" y="{}" text-anchor="middle" font-size="10" fill="#aaa6bc" letter-spacing="1">KILLS</text>"##,
            float(y + 26.0)
        );
        let kills = seat
            .kills
            .map_or_else(|| "—".to_owned(), |kills| kills.to_string());
        let _ = writeln!(
            out,
            r#"      <text x="1132" y="{}" text-anchor="middle" font-size="25" font-weight="bold">{kills}</text>"#,
            float(y + 55.0)
        );
    }
    out.push('\n');
    let dash = |value: Option<i64>| value.map_or_else(|| "—".to_owned(), |value| value.to_string());
    let _ = writeln!(
        out,
        r##"    <text x="32" y="{}" font-size="17" fill="#d2cddd">{} PLAYERS  ·  {} TURNS  ·  {} MINUTES</text>"##,
        body_height + 141,
        seats.len(),
        dash(game.turns),
        dash(game.duration_minutes)
    );
    let _ = writeln!(
        out,
        r##"    <line x1="32" x2="1168" y1="{0}" y2="{0}" stroke="#383446"/>"##,
        body_height + 160
    );
    let _ = writeln!(
        out,
        r##"    <text x="32" y="{}" font-size="11" font-weight="bold" fill="#aaa6bc" letter-spacing="2">TABLE TALK</text>"##,
        body_height + 185
    );
    for (index, line) in notes.iter().enumerate() {
        let _ = writeln!(
            out,
            r##"      <text x="32" y="{}" font-family="DejaVu Sans Mono" font-size="18" fill="#d2cddd">{}</text>"##,
            body_height + 213 + int(index) * 24,
            escape(line)
        );
    }
    out.push_str("  </g>\n</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_like_the_template() {
        assert_eq!(lines("one two three", 7, 2), ["one two", "three"]);
        assert_eq!(lines("one two three four", 7, 1), ["one t……"]);
        assert_eq!(lines("supercalifragilistic", 8, 2), ["superca…"]);
        assert_eq!(float(88.0), "88.0");
        assert_eq!(float(126.5), "126.5");
        assert_eq!(escape("<a & 'b'>\u{1}"), "&lt;a &amp; &#39;b&#39;&gt;");
    }
}
