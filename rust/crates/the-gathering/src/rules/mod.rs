//! The Magic: The Gathering Comprehensive Rules: parsing the text release Wizards of the
//! Coast publishes, storing it, and the lookups AI rulings make in it.
//!
//! [`sync`] finds and installs new releases. The stored document is one row per numbered
//! rule (sections such as `7`, chapters such as `702`, rules such as `702.19`, and subrules
//! such as `702.19c`, with their examples) plus the glossary.

pub mod sync;

use std::collections::HashSet;
use std::sync::LazyLock;

use serde::Serialize;
use time::Date;
use time::macros::format_description;

use crate::db::{self, IsoDate, Pool, UtcDateTime};
use crate::regex::{Regex, compile};

/// `100.1.` or `100.1a`: a rule or subrule and its text.
static RULE: LazyLock<Regex> = LazyLock::new(|| compile(r"^(\d{3}\.\d+[a-z]?)\.?\s+(\S.*)$"));
/// `1. Game Concepts` or `100. General`: a section or chapter heading.
static HEADING: LazyLock<Regex> = LazyLock::new(|| compile(r"^(\d|\d{3})\.\s+(\S.*)$"));
/// A rule number as a reader would write it, such as `702.19c` or `CR 702.19.`.
static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^(?i:cr\s*|rule\s*)?(\d|\d{3}|\d{3}\.\d+[a-z]?)\.?$"));
/// Rule numbers cited in prose.
static CITATION: LazyLock<Regex> = LazyLock::new(|| compile(r"\b(\d{3}\.\d+[a-z]?)\b"));

/// Results [`search`] returns at most.
pub const MAX_SEARCH_RESULTS: usize = 25;
/// Rules one [`lookup`] number expands to at most.
const MAX_EXPANSION: usize = 80;

/// One numbered rule or heading.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Rule {
    /// `7`, `702`, `702.19`, or `702.19c`.
    pub number: String,
    /// The rule's text, with its examples on following lines.
    pub text: String,
}

/// One glossary entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GlossaryEntry {
    /// The term.
    pub term: String,
    /// Its definition; numbered meanings are on separate lines.
    pub definition: String,
}

/// A parsed release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// "These rules are effective as of ...".
    pub effective_on: Option<IsoDate>,
    /// Every rule and heading in order.
    pub rules: Vec<Rule>,
    /// The glossary in order.
    pub glossary: Vec<GlossaryEntry>,
}

fn effective_date(line: &str) -> Option<IsoDate> {
    let (_, rest) = line.split_once("effective as of ")?;
    let date = rest.trim().trim_end_matches('.');
    Date::parse(
        date,
        format_description!("[month repr:long] [day padding:none], [year]"),
    )
    .ok()
    .map(IsoDate)
}

/// Parses the text release: the contents list, the numbered rules up to the glossary, and
/// the glossary up to the credits. `None` when the text does not have that layout.
pub fn parse(text: &str) -> Option<Document> {
    let lines: Vec<&str> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .collect();
    let effective_on = lines.iter().find_map(|line| effective_date(line));
    // The contents list ends with "Glossary" and "Credits"; the rules follow it, and the
    // glossary and credits sections follow the rules.
    let contents_end = lines.iter().position(|line| *line == "Credits")?;
    let body = lines.get(contents_end + 1..)?;
    let glossary_start = body.iter().position(|line| *line == "Glossary")?;
    let rule_lines = body.get(..glossary_start)?;
    let after = body.get(glossary_start + 1..)?;
    let glossary_end = after
        .iter()
        .position(|line| *line == "Credits")
        .unwrap_or(after.len());

    let mut rules: Vec<Rule> = Vec::new();
    for line in rule_lines.iter().filter(|line| !line.is_empty()) {
        let captures = RULE.captures(line).or_else(|| HEADING.captures(line));
        match (captures, rules.last_mut()) {
            (Some(captures), _) => rules.push(Rule {
                number: captures.get(1)?.as_str().to_owned(),
                text: captures.get(2)?.as_str().to_owned(),
            }),
            // Examples and other continuation lines belong to the rule above them.
            (None, Some(rule)) => {
                rule.text.push('\n');
                rule.text.push_str(line);
            }
            (None, None) => {}
        }
    }

    let mut glossary = Vec::new();
    for entry in after.get(..glossary_end)?.split(|line| line.is_empty()) {
        if let [term, definition @ ..] = entry
            && !definition.is_empty()
        {
            glossary.push(GlossaryEntry {
                term: (*term).to_owned(),
                definition: definition.join("\n").replace('\u{2028}', "\n"),
            });
        }
    }
    if rules.is_empty() || glossary.is_empty() {
        return None;
    }
    Some(Document {
        effective_on,
        rules,
        glossary,
    })
}

/// The installed release and the updater's last check.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The installed document's URL.
    pub source_url: Option<String>,
    /// Its effective date.
    pub effective_on: Option<IsoDate>,
    /// Rules and headings installed.
    pub rule_count: i64,
    /// Glossary entries installed.
    pub glossary_count: i64,
    /// When it was installed.
    pub installed_at: Option<UtcDateTime>,
    /// The last check for a new release.
    pub checked_at: Option<UtcDateTime>,
    /// Why the last check failed.
    pub last_error: Option<String>,
}

/// The installed release, or the default (nothing installed, never checked).
pub async fn status(pool: &Pool) -> Result<Status, sqlx::Error> {
    Ok(sqlx::query_as!(
        Status,
        r#"SELECT source_url, effective_on AS "effective_on: IsoDate", rule_count, glossary_count,
                  installed_at AS "installed_at: UtcDateTime", checked_at AS "checked_at: UtcDateTime",
                  last_error
           FROM comprehensive_rules_updates WHERE id = 1"#
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or_default())
}

/// Replaces the installed rules with `document`, downloaded from `source_url`.
pub async fn install(
    pool: &Pool,
    source_url: &str,
    document: &Document,
) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    let rule_count = i64::try_from(document.rules.len()).unwrap_or(i64::MAX);
    let glossary_count = i64::try_from(document.glossary.len()).unwrap_or(i64::MAX);
    let mut tx = db::begin(pool).await?;
    sqlx::query!("DELETE FROM comprehensive_rules")
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM comprehensive_rules_glossary")
        .execute(&mut *tx)
        .await?;
    for rule in &document.rules {
        sqlx::query!(
            "INSERT INTO comprehensive_rules (number, text) VALUES (?, ?)",
            rule.number,
            rule.text
        )
        .execute(&mut *tx)
        .await?;
    }
    for entry in &document.glossary {
        sqlx::query!(
            "INSERT INTO comprehensive_rules_glossary (term, definition) VALUES (?, ?)",
            entry.term,
            entry.definition
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        "INSERT INTO comprehensive_rules_updates
            (id, source_url, effective_on, rule_count, glossary_count, installed_at, checked_at, last_error)
         VALUES (1, ?, ?, ?, ?, ?, ?, NULL)
         ON CONFLICT (id) DO UPDATE SET source_url = excluded.source_url,
            effective_on = excluded.effective_on, rule_count = excluded.rule_count,
            glossary_count = excluded.glossary_count, installed_at = excluded.installed_at,
            checked_at = excluded.checked_at, last_error = NULL",
        source_url,
        document.effective_on,
        rule_count,
        glossary_count,
        now,
        now
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Records a check that installed nothing: `error` says why it failed, `None` that the
/// installed release is current.
pub async fn record_check(pool: &Pool, error: Option<&str>) -> Result<(), sqlx::Error> {
    let now = UtcDateTime::now();
    let mut tx = db::begin(pool).await?;
    sqlx::query!(
        "INSERT INTO comprehensive_rules_updates (id, checked_at, last_error) VALUES (1, ?, ?)
         ON CONFLICT (id) DO UPDATE SET checked_at = excluded.checked_at, last_error = excluded.last_error",
        now,
        error
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// The section and chapter headings, the table of contents.
pub async fn contents(pool: &Pool) -> Result<Vec<Rule>, sqlx::Error> {
    sqlx::query_as!(
        Rule,
        "SELECT number, text FROM comprehensive_rules WHERE number NOT LIKE '%.%' ORDER BY position"
    )
    .fetch_all(pool)
    .await
}

/// A rule number in a reader's notation (`CR 702.19c`, `702.19.`), canonicalized.
pub fn canonical_number(number: &str) -> Option<String> {
    NUMBER
        .captures(number.trim())
        .and_then(|captures| captures.get(1))
        .map(|number| number.as_str().to_lowercase())
}

/// The rules `number` names: a section's chapter headings, a chapter's heading and its
/// rules (without subrules), a rule and its subrules, or one subrule. `None` for a number
/// that is not a rule number; empty when no installed rule has it.
pub async fn lookup(pool: &Pool, number: &str) -> Result<Option<Vec<Rule>>, sqlx::Error> {
    let Some(number) = canonical_number(number) else {
        return Ok(None);
    };
    let pattern = match number.split_once('.') {
        None if number.len() == 1 => format!("{number}[0-9][0-9]"),
        None => format!("{number}.[0-9]*"),
        Some((_, rest)) if rest.ends_with(|c: char| c.is_ascii_digit()) => {
            format!("{number}[a-z]")
        }
        Some(_) => number.clone(),
    };
    let chapter = number.len() == 3;
    let rules = sqlx::query_as!(
        Rule,
        "SELECT number, text FROM comprehensive_rules WHERE number = ? OR number GLOB ? ORDER BY position",
        number,
        pattern
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    // A chapter lists its rules; their subrules are a lookup of their own.
    .filter(|rule| !chapter || !rule.number.ends_with(|c: char| c.is_ascii_lowercase()))
    .take(MAX_EXPANSION)
    .collect();
    Ok(Some(rules))
}

/// The installed rules among the numbers `text` cites, in order of first citation.
pub async fn cited(pool: &Pool, text: &str, limit: usize) -> Result<Vec<Rule>, sqlx::Error> {
    let mut seen = HashSet::new();
    let mut rules = Vec::new();
    for number in CITATION.captures_iter(text).filter_map(|c| c.get(1)) {
        let number = number.as_str();
        if rules.len() >= limit || !seen.insert(number.to_owned()) {
            continue;
        }
        if let Some(rule) = sqlx::query_as!(
            Rule,
            "SELECT number, text FROM comprehensive_rules WHERE number = ?",
            number
        )
        .fetch_optional(pool)
        .await?
        {
            rules.push(rule);
        }
    }
    Ok(rules)
}

/// A [`search`] result.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Hit {
    /// A numbered rule.
    Rule {
        /// Its number.
        number: String,
        /// Its text.
        text: String,
    },
    /// A glossary entry.
    Glossary {
        /// The term.
        term: String,
        /// Its definition.
        definition: String,
    },
}

const STOP_WORDS: [&str; 46] = [
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "do", "does", "for", "from",
    "has", "have", "how", "i", "if", "in", "into", "is", "it", "its", "my", "of", "on", "or",
    "rule", "rules", "so", "that", "the", "their", "then", "there", "they", "this", "to", "what",
    "when", "where", "which", "while", "who", "with",
];

/// A crude stem, so `tramples`, `trampled`, and `trampling` meet `trample`.
fn stem(word: &str) -> String {
    let word = word.trim_end_matches("'s").trim_end_matches('\'');
    let length = word.chars().count();
    let base = if length > 4
        && let Some(base) = word.strip_suffix("ies")
    {
        format!("{base}y")
    } else if length > 6
        && let Some(base) = word.strip_suffix("ing")
    {
        base.to_owned()
    } else if length > 5
        && let Some(base) = word.strip_suffix("ed")
    {
        base.to_owned()
    } else if length > 3 && word.ends_with('s') && !word.ends_with("ss") {
        word.trim_end_matches('s').to_owned()
    } else {
        word.to_owned()
    };
    match base.strip_suffix('e') {
        Some(stripped) if stripped.chars().count() >= 4 => stripped.to_owned(),
        _ => base,
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .map(stem)
        .collect()
}

fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty() && !STOP_WORDS.contains(word))
    {
        let term = stem(word);
        if !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms.truncate(12);
    terms
}

/// Whether a document word matches a query term: the same stem, or a longer word starting
/// with a term of four letters or more (`exil` finds `exile`).
fn matches(word: &str, term: &str) -> bool {
    word == term || (term.chars().count() >= 4 && word.starts_with(term))
}

/// Ranks texts for `terms` with BM25 and returns the indexes of the best `limit`.
fn rank(texts: &[Vec<String>], terms: &[String], boosts: &[f64], limit: usize) -> Vec<usize> {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let count = f64::from(u32::try_from(texts.len()).unwrap_or(u32::MAX)).max(1.0);
    let length = |text: &Vec<String>| f64::from(u32::try_from(text.len()).unwrap_or(u32::MAX));
    let average = texts.iter().map(length).sum::<f64>() / count;
    let frequencies: Vec<Vec<f64>> = texts
        .iter()
        .map(|text| {
            terms
                .iter()
                .map(|term| {
                    let hits = text.iter().filter(|word| matches(word, term)).count();
                    f64::from(u32::try_from(hits).unwrap_or(u32::MAX))
                })
                .collect()
        })
        .collect();
    let idf: Vec<f64> = (0..terms.len())
        .map(|index| {
            let with = frequencies
                .iter()
                .filter(|row| row.get(index).is_some_and(|tf| *tf > 0.0))
                .count();
            let with = f64::from(u32::try_from(with).unwrap_or(u32::MAX));
            ((count - with + 0.5) / (with + 0.5) + 1.0).ln()
        })
        .collect();
    let mut scored: Vec<(usize, f64)> = texts
        .iter()
        .zip(&frequencies)
        .enumerate()
        .filter_map(|(index, (text, row))| {
            let norm = K1 * (1.0 - B + B * length(text) / average.max(1.0));
            let score: f64 = row
                .iter()
                .zip(&idf)
                .map(|(tf, idf)| idf * tf * (K1 + 1.0) / (tf + norm))
                .sum();
            (score > 0.0).then(|| (index, score * boosts.get(index).copied().unwrap_or(1.0)))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    scored
        .into_iter()
        .take(limit)
        .map(|(index, _)| index)
        .collect()
}

/// Keyword search over the rules and the glossary, best first. A glossary entry whose term
/// is one of the query's words or the whole query ranks above rules that merely use it.
pub async fn search(pool: &Pool, query: &str, limit: usize) -> Result<Vec<Hit>, sqlx::Error> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let rules = sqlx::query_as!(
        Rule,
        "SELECT number, text FROM comprehensive_rules ORDER BY position"
    )
    .fetch_all(pool)
    .await?;
    let glossary = sqlx::query_as!(
        GlossaryEntry,
        "SELECT term, definition FROM comprehensive_rules_glossary ORDER BY position"
    )
    .fetch_all(pool)
    .await?;
    let query_words = words(query).join(" ");
    let mut texts = Vec::with_capacity(rules.len() + glossary.len());
    let mut boosts = Vec::with_capacity(rules.len() + glossary.len());
    for rule in &rules {
        texts.push(words(&rule.text));
        boosts.push(1.0);
    }
    for entry in &glossary {
        let term = words(&entry.term).join(" ");
        let named = term == query_words || terms.contains(&term);
        texts.push(words(&format!("{} {}", entry.term, entry.definition)));
        boosts.push(if named { 3.0 } else { 1.0 });
    }
    let hits = rank(&texts, &terms, &boosts, limit.clamp(1, MAX_SEARCH_RESULTS))
        .into_iter()
        .filter_map(|index| match index.checked_sub(rules.len()) {
            None => rules.get(index).map(|rule| Hit::Rule {
                number: rule.number.clone(),
                text: rule.text.clone(),
            }),
            Some(index) => glossary.get(index).map(|entry| Hit::Glossary {
                term: entry.term.clone(),
                definition: entry.definition.clone(),
            }),
        })
        .collect();
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_inflections_together() {
        for word in ["tramples", "trampled", "trampling", "Trample's"] {
            assert_eq!(stem(&word.to_lowercase()), stem("trample"), "{word}");
        }
        assert_eq!(stem("copies"), stem("copy"));
        assert_eq!(stem("attacking"), stem("attack"));
        assert_eq!(stem("exiled"), stem("exile"));
        assert_eq!(stem("loss"), "loss");
        assert_eq!(stem("rules"), "rule");
        assert!(matches("exiled", &stem("exile")));
        assert!(!matches("untap", "tap"));
    }

    #[test]
    fn canonicalizes_rule_numbers() {
        assert_eq!(canonical_number("CR 702.19c").as_deref(), Some("702.19c"));
        assert_eq!(canonical_number("702.19.").as_deref(), Some("702.19"));
        assert_eq!(canonical_number("rule 903").as_deref(), Some("903"));
        assert_eq!(canonical_number("7").as_deref(), Some("7"));
        for invalid in ["70", "7022", "trample", "702.19cc", "702.x"] {
            assert_eq!(canonical_number(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn rejects_text_without_the_rules_layout() {
        assert_eq!(parse("Just some text\nCredits\nNothing"), None);
        assert_eq!(parse(""), None);
    }
}
