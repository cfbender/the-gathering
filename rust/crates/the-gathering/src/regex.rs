//! Compiling the crate's literal regular expressions.

pub use regex::Regex;

/// Compiles a pattern that is a literal in this crate.
///
/// Every caller passes a string literal, and the tests exercise each pattern, so a compile
/// failure is a programming error caught before release. This is the one place the crate
/// allows `expect` (the same rule as lotus).
#[allow(clippy::expect_used)]
pub fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("literal regular expression compiles")
}
