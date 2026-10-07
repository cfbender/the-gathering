//! Validation failures and the checks that produce them.
//!
//! A [`ValidationError`] holds messages per field, in the order the checks added them, plus
//! per-row errors for list fields (a game's seats). The API renders it as the 422 body
//! `{"errors": {"field": ["message"], "seats": [{}, {"player": ["message"]}]}}`; row `i` of a
//! list is always input row `i`, so the SPA can number rows by index. `Display` gives one
//! readable sentence for logs and import messages.

use std::fmt;

use serde_json::{Map, Value, json};

/// Field messages and per-row errors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidationError {
    fields: Vec<(String, Vec<String>)>,
    rows: Vec<(String, Vec<ValidationError>)>,
}

impl ValidationError {
    /// No errors.
    pub fn new() -> Self {
        Self::default()
    }

    /// One error.
    pub fn single(field: &str, message: impl Into<String>) -> Self {
        let mut errors = Self::new();
        errors.add(field, message);
        errors
    }

    /// Adds `message` to `field`, after its earlier messages.
    pub fn add(&mut self, field: &str, message: impl Into<String>) {
        let message = message.into();
        match self.fields.iter_mut().find(|(name, _)| name == field) {
            Some((_, messages)) => messages.push(message),
            None => self.fields.push((field.to_owned(), vec![message])),
        }
    }

    /// Sets the per-row errors of a list field (ignored when no row has errors).
    pub fn set_rows(&mut self, field: &str, rows: Vec<ValidationError>) {
        if rows.iter().all(ValidationError::is_empty) {
            return;
        }
        match self.rows.iter_mut().find(|(name, _)| name == field) {
            Some((_, existing)) => *existing = rows,
            None => self.rows.push((field.to_owned(), rows)),
        }
    }

    /// Whether there are no errors at all.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
            && self
                .rows
                .iter()
                .all(|(_, rows)| rows.iter().all(Self::is_empty))
    }

    /// Whether `field` has a message.
    pub fn has(&self, field: &str) -> bool {
        self.fields.iter().any(|(name, _)| name == field)
    }

    /// Messages for `field`, oldest first.
    pub fn messages(&self, field: &str) -> &[String] {
        self.fields
            .iter()
            .find(|(name, _)| name == field)
            .map_or(&[], |(_, messages)| messages.as_slice())
    }

    /// Per-row errors of a list field (empty when no row has errors).
    pub fn rows(&self, field: &str) -> &[ValidationError] {
        self.rows
            .iter()
            .find(|(name, _)| name == field)
            .map_or(&[], |(_, rows)| rows.as_slice())
    }

    /// `Ok(())` when empty.
    pub fn into_result(self) -> Result<(), ValidationError> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }

    /// The JSON object. A list field with row errors renders its rows (so indexes line up
    /// with the input); its own messages appear only in [`Display`](fmt::Display).
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        for (field, messages) in &self.fields {
            object.insert(field.clone(), json!(messages));
        }
        for (field, rows) in &self.rows {
            let rows: Vec<Value> = rows.iter().map(ValidationError::to_json).collect();
            object.insert(field.clone(), Value::Array(rows));
        }
        Value::Object(object)
    }

    fn sentences(&self, prefix: &str, out: &mut Vec<String>) {
        for (field, messages) in &self.fields {
            for message in messages {
                out.push(format!("{prefix}{} {message}", humanize(field)));
            }
        }
        for (field, rows) in &self.rows {
            let label = humanize(field.strip_suffix('s').unwrap_or(field));
            for (index, row) in rows.iter().enumerate() {
                row.sentences(&format!("{prefix}{label} {}: ", index + 1), out);
            }
        }
    }
}

/// `display_name` as `Display name`; `player_id` as `Player`.
fn humanize(field: &str) -> String {
    let words = field.strip_suffix("_id").unwrap_or(field).replace('_', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl fmt::Display for ValidationError {
    /// `Name can't be blank; Seat 2: Player has already been taken`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sentences = Vec::new();
        self.sentences("", &mut sentences);
        f.write_str(&sentences.join("; "))
    }
}

impl std::error::Error for ValidationError {}

/// `"has already been taken"`.
pub const TAKEN: &str = "has already been taken";
/// `"does not exist"`.
pub const DOES_NOT_EXIST: &str = "does not exist";

/// Checks values and collects their failures.
#[derive(Debug, Default)]
pub struct Validator {
    /// Failures so far.
    pub errors: ValidationError,
}

impl Validator {
    /// No failures yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a failure.
    pub fn add_error(&mut self, field: &str, message: impl Into<String>) {
        self.errors.add(field, message);
    }

    /// `"can't be blank"` when missing or blank (unless the field already failed).
    pub fn required<T>(&mut self, field: &str, value: Option<&T>) -> bool
    where
        T: AsRef<str> + ?Sized,
    {
        let present = value.is_some_and(|value| !value.as_ref().trim().is_empty());
        if !present && !self.errors.has(field) {
            self.errors.add(field, "can't be blank");
        }
        present
    }

    /// `"can't be blank"` when missing, for non-string values.
    pub fn required_value<T>(&mut self, field: &str, value: Option<&T>) -> bool {
        if value.is_none() && !self.errors.has(field) {
            self.errors.add(field, "can't be blank");
        }
        value.is_some()
    }

    /// Length in characters within `min..=max`.
    pub fn length(
        &mut self,
        field: &str,
        value: Option<&str>,
        min: Option<usize>,
        max: Option<usize>,
    ) {
        let Some(value) = value else { return };
        let count = value.chars().count();
        if let Some(min) = min.filter(|min| count < *min) {
            self.errors
                .add(field, format!("should be at least {min} character(s)"));
        } else if let Some(max) = max.filter(|max| count > *max) {
            self.errors
                .add(field, format!("should be at most {max} character(s)"));
        }
    }

    /// At most `max` bytes.
    pub fn max_bytes(&mut self, field: &str, value: Option<&str>, max: usize) {
        if value.is_some_and(|value| value.len() > max) {
            self.errors
                .add(field, format!("should be at most {max} byte(s)"));
        }
    }

    /// Matches `pattern`.
    pub fn format(
        &mut self,
        field: &str,
        value: Option<&str>,
        pattern: &regex::Regex,
        message: &str,
    ) {
        if value.is_some_and(|value| !pattern.is_match(value)) {
            self.errors.add(field, message);
        }
    }

    /// One of `allowed`.
    pub fn inclusion(&mut self, field: &str, value: Option<&str>, allowed: &[&str]) {
        if value.is_some_and(|value| !allowed.contains(&value)) {
            self.errors.add(field, "is invalid");
        }
    }

    /// Greater than `bound`.
    pub fn greater_than(&mut self, field: &str, value: Option<i64>, bound: i64) {
        if value.is_some_and(|value| value <= bound) {
            self.errors
                .add(field, format!("must be greater than {bound}"));
        }
    }

    /// At least `bound`.
    pub fn at_least(&mut self, field: &str, value: Option<i64>, bound: i64) {
        if value.is_some_and(|value| value < bound) {
            self.errors
                .add(field, format!("must be greater than or equal to {bound}"));
        }
    }

    /// At most `bound`.
    pub fn at_most(&mut self, field: &str, value: Option<i64>, bound: i64) {
        if value.is_some_and(|value| value > bound) {
            self.errors
                .add(field, format!("must be less than or equal to {bound}"));
        }
    }

    /// Whether nothing failed.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// `Ok(())` or the failures.
    pub fn finish(self) -> Result<(), ValidationError> {
        self.errors.into_result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_messages_in_order_and_rows_by_index() {
        let mut errors = ValidationError::new();
        errors.add("seats", "must contain between 2 and 10 players");
        errors.add("seats", "cannot contain the same player twice");
        assert_eq!(
            errors.to_json(),
            json!({ "seats": ["must contain between 2 and 10 players", "cannot contain the same player twice"] })
        );

        let mut rows = ValidationError::single("name", "can't be blank");
        rows.set_rows(
            "seats",
            vec![
                ValidationError::new(),
                ValidationError::single("player_id", TAKEN),
            ],
        );
        assert_eq!(
            rows.to_json(),
            json!({ "name": ["can't be blank"], "seats": [{}, { "player_id": ["has already been taken"] }] })
        );
        assert_eq!(
            rows.to_string(),
            "Name can't be blank; Seat 2: Player has already been taken"
        );
        assert!(!rows.is_empty());
        rows.set_rows("other", vec![ValidationError::new()]);
        assert!(rows.rows("other").is_empty());
    }

    #[test]
    fn row_errors_win_over_list_messages_in_json_but_not_in_display() {
        let mut errors = ValidationError::single("seats", "must contain between 2 and 10 players");
        errors.set_rows(
            "seats",
            vec![ValidationError::single(
                "kills",
                "must be greater than or equal to 0",
            )],
        );
        assert_eq!(
            errors.to_json(),
            json!({ "seats": [{ "kills": ["must be greater than or equal to 0"] }] })
        );
        assert_eq!(
            errors.to_string(),
            "Seats must contain between 2 and 10 players; Seat 1: Kills must be greater than or equal to 0"
        );
    }

    #[test]
    fn validator_checks() {
        let mut validator = Validator::new();
        assert!(!validator.required("name", Some("  ")));
        validator.length("username", Some("ab"), Some(3), Some(20));
        validator.inclusion("role", Some("owner"), &["admin", "member"]);
        validator.at_least("kills", Some(-1), 0);
        let errors = validator.finish().unwrap_err();
        assert_eq!(errors.messages("name"), ["can't be blank"]);
        assert_eq!(
            errors.messages("username"),
            ["should be at least 3 character(s)"]
        );
        assert_eq!(errors.messages("role"), ["is invalid"]);
        assert_eq!(
            errors.messages("kills"),
            ["must be greater than or equal to 0"]
        );
    }
}
