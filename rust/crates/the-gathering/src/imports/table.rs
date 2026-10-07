//! Delimited text (CSV/TSV) parsing with `NimbleCSV`'s rules, as `NimbleCSV.define(..., escape:
//! "\"")` parsers read pasted and uploaded files.
//!
//! The `csv` crate does the splitting and unquoting; it is more lenient than `NimbleCSV`,
//! so a pre-pass reproduces `NimbleCSV.ParseError`'s conditions and messages: a quote that
//! does not start a field, text after a closing quote, and an unclosed quote at the end.
//!
//! Rows carry their 1-based starting line. Like `NimbleCSV`, lines end at `\n` or `\r\n`
//! (a lone `\r` is data) and rows may have any number of fields. Two deliberate
//! differences: blank lines yield no row (every caller skipped blank rows anyway), and
//! a row's number is the physical line it starts on, which `NimbleCSV`-based numbering
//! only matched when no quoted cell spanned lines.

use std::fmt::Write as _;

/// One parsed row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The line the row starts on.
    pub line: i64,
    /// Its fields, unquoted.
    pub fields: Vec<String>,
}

/// `inspect/1` of a string, as `NimbleCSV` quotes lines in its errors.
pub fn inspect_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{1b}' => out.push_str("\\e"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '\0' => out.push_str("\\0"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\x{:02X}", u32::from(ch));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn unexpected(line: &str) -> String {
    format!("unexpected escape character \" in {}", inspect_string(line))
}

/// Lines including their terminator, split after every `\n`.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive('\n')
}

fn at_line_end(rest: &str) -> bool {
    rest.is_empty() || rest == "\n" || rest == "\r\n"
}

/// `NimbleCSV`'s parse errors, or `Ok` when it would parse `text`.
fn validate(text: &str, separator: char) -> Result<(), String> {
    let mut quoted = false;
    for line in lines(text) {
        let mut rest = line;
        loop {
            if quoted {
                let Some(position) = rest.find('"') else {
                    break;
                };
                let after = rest.get(position + 1..).unwrap_or_default();
                if let Some(next) = after.strip_prefix('"') {
                    rest = next;
                } else if let Some(next) = after.strip_prefix(separator) {
                    quoted = false;
                    rest = next;
                } else if at_line_end(after) {
                    quoted = false;
                    break;
                } else {
                    return Err(unexpected(rest));
                }
            } else {
                // `rest` always starts at a field here.
                let Some(position) = rest.find('"') else {
                    break;
                };
                let prefix = rest.get(..position).unwrap_or_default();
                if position == 0 || prefix.ends_with(separator) {
                    quoted = true;
                    rest = rest.get(position + 1..).unwrap_or_default();
                } else {
                    return Err(unexpected(rest));
                }
            }
        }
    }
    if quoted {
        Err("expected escape character \" but reached the end of file".to_owned())
    } else {
        Ok(())
    }
}

/// Parses `text` split on `separator` (an ASCII character).
pub fn parse(text: &str, separator: u8) -> Result<Vec<Row>, String> {
    validate(text, char::from(separator))?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(separator)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    let mut record = csv::StringRecord::new();
    // The reader positions a record before the blank lines it skips, so lines are counted
    // from the record's first byte instead.
    let (mut counted_to, mut line) = (0_usize, 1_i64);
    loop {
        match reader.read_record(&mut record) {
            Ok(false) => break,
            Ok(true) => {
                let mut start = record
                    .position()
                    .and_then(|position| usize::try_from(position.byte()).ok())
                    .unwrap_or(counted_to)
                    .max(counted_to);
                while let Some(rest) = text.get(start..) {
                    if rest.starts_with('\n') {
                        start += 1;
                    } else if rest.starts_with("\r\n") {
                        start += 2;
                    } else {
                        break;
                    }
                }
                let newlines = text
                    .get(counted_to..start)
                    .map_or(0, |skipped| skipped.matches('\n').count());
                line += i64::try_from(newlines).unwrap_or(0);
                counted_to = start;
                let mut fields: Vec<String> = record.iter().map(str::to_owned).collect();
                // `\r\n` endings: the `\r` belongs to the terminator, not the last field.
                if let Some(last) = fields.last_mut()
                    && last.ends_with('\r')
                {
                    last.pop();
                }
                rows.push(Row { line, fields });
            }
            Err(error) => return Err(format!("could not parse file: {error}")),
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(rows: &[Row]) -> Vec<(i64, Vec<&str>)> {
        rows.iter()
            .map(|row| (row.line, row.fields.iter().map(String::as_str).collect()))
            .collect()
    }

    #[test]
    fn parses_like_nimble_csv() {
        let rows = parse(
            "a,\"b,c\",\"say \"\"hi\"\"\"\r\n\nd,\"multi\nline\"\ne\n",
            b',',
        )
        .unwrap();
        assert_eq!(
            fields(&rows),
            [
                (1, vec!["a", "b,c", "say \"hi\""]),
                (3, vec!["d", "multi\nline"]),
                (5, vec!["e"])
            ]
        );
        assert_eq!(
            parse("a\tb\n", b'\t').unwrap()[0].fields,
            ["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn reports_nimble_csv_errors() {
        assert_eq!(
            parse("a,b\"c\n", b',').unwrap_err(),
            "unexpected escape character \" in \"a,b\\\"c\\n\""
        );
        assert_eq!(
            parse("\"ab\"c\n", b',').unwrap_err(),
            "unexpected escape character \" in \"ab\\\"c\\n\""
        );
        assert_eq!(
            parse("a,\"open\n", b',').unwrap_err(),
            "expected escape character \" but reached the end of file"
        );
        assert!(parse("", b',').unwrap().is_empty());
    }
}
