//! A small stand-in for `Ecto.Changeset`: casts JSON params with Ecto's rules and collects
//! validation errors with Ecto's exact messages, so forms show the same text as before.
//!
//! Casting follows `Ecto.Changeset.cast/4`: blank and whitespace-only strings become `nil`,
//! non-blank strings are kept untrimmed, integers accept numeric strings, booleans accept
//! `"true"`/`"false"`/`"1"`/`"0"`, and anything else is `"is invalid"`.

use serde_json::{Map, Value};

use crate::db::{IsoDate, UtcDateTime};
use crate::error::Errors;

/// A cast param: absent (keep the stored value) or present (possibly `nil`).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Change<T> {
    /// The key was not in the params, or failed to cast.
    #[default]
    Unchanged,
    /// The key was present; `None` for `null` or blank strings.
    Set(Option<T>),
}

impl<T> Change<T> {
    /// The new value, falling back to `current` when unchanged.
    pub fn or(self, current: Option<T>) -> Option<T> {
        match self {
            Self::Unchanged => current,
            Self::Set(value) => value,
        }
    }

    /// Whether the key was given.
    pub fn is_set(&self) -> bool {
        matches!(self, Self::Set(_))
    }

    /// Maps the value.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Change<U> {
        match self {
            Self::Unchanged => Change::Unchanged,
            Self::Set(value) => Change::Set(value.map(f)),
        }
    }
}

/// Ecto's empty check: `nil` or a string that trims to `""`.
fn blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// Collects cast and validation errors.
#[derive(Debug, Default)]
pub struct Changeset<'a> {
    params: Option<&'a Map<String, Value>>,
    /// Errors so far.
    pub errors: Errors,
}

impl<'a> Changeset<'a> {
    /// Casts from `params` (a JSON object; anything else casts nothing).
    pub fn new(params: &'a Value) -> Self {
        Self {
            params: params.as_object(),
            errors: Errors::new(),
        }
    }

    /// A changeset with no params, for validating programmatic values.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The raw param.
    pub fn raw(&self, field: &str) -> Option<&'a Value> {
        self.params.and_then(|params| params.get(field))
    }

    fn cast<T>(&mut self, field: &str, convert: impl FnOnce(&Value) -> Option<T>) -> Change<T> {
        match self.raw(field) {
            None => Change::Unchanged,
            Some(value) if blank(value) => Change::Set(None),
            Some(value) => {
                if let Some(cast) = convert(value) {
                    Change::Set(Some(cast))
                } else {
                    self.errors.add(field, "is invalid");
                    Change::Unchanged
                }
            }
        }
    }

    /// A `:string` field.
    pub fn string(&mut self, field: &str) -> Change<String> {
        self.cast(field, |value| value.as_str().map(str::to_owned))
    }

    /// An `:integer` field.
    pub fn integer(&mut self, field: &str) -> Change<i64> {
        self.cast(field, cast_integer)
    }

    /// A `:boolean` field.
    pub fn boolean(&mut self, field: &str) -> Change<bool> {
        self.cast(field, |value| match value {
            Value::Bool(flag) => Some(*flag),
            Value::String(text) => match text.as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            },
            _ => None,
        })
    }

    /// A `:utc_datetime` field.
    pub fn datetime(&mut self, field: &str) -> Change<UtcDateTime> {
        self.cast(field, |value| value.as_str().and_then(UtcDateTime::parse))
    }

    /// A `:date` field.
    pub fn date(&mut self, field: &str) -> Change<IsoDate> {
        self.cast(field, |value| value.as_str().and_then(IsoDate::parse))
    }

    /// Adds an error.
    pub fn add_error(&mut self, field: &str, message: impl Into<String>) {
        self.errors.add(field, message);
    }

    /// `validate_required`: `"can't be blank"` when missing or blank.
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

    /// `validate_required` for non-string values.
    pub fn required_value<T>(&mut self, field: &str, value: Option<&T>) -> bool {
        if value.is_none() && !self.errors.has(field) {
            self.errors.add(field, "can't be blank");
        }
        value.is_some()
    }

    /// `validate_length` counting graphemes (approximated by characters).
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

    /// `validate_length(..., count: :bytes)` maximum.
    pub fn max_bytes(&mut self, field: &str, value: Option<&str>, max: usize) {
        if value.is_some_and(|value| value.len() > max) {
            self.errors
                .add(field, format!("should be at most {max} byte(s)"));
        }
    }

    /// `validate_format`.
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

    /// `validate_inclusion`.
    pub fn inclusion(&mut self, field: &str, value: Option<&str>, allowed: &[&str]) {
        if value.is_some_and(|value| !allowed.contains(&value)) {
            self.errors.add(field, "is invalid");
        }
    }

    /// `validate_number(greater_than: n)`.
    pub fn greater_than(&mut self, field: &str, value: Option<i64>, bound: i64) {
        if value.is_some_and(|value| value <= bound) {
            self.errors
                .add(field, format!("must be greater than {bound}"));
        }
    }

    /// `validate_number(greater_than_or_equal_to: n)`.
    pub fn at_least(&mut self, field: &str, value: Option<i64>, bound: i64) {
        if value.is_some_and(|value| value < bound) {
            self.errors
                .add(field, format!("must be greater than or equal to {bound}"));
        }
    }

    /// `validate_number(less_than_or_equal_to: n)`.
    pub fn at_most(&mut self, field: &str, value: Option<i64>, bound: i64) {
        if value.is_some_and(|value| value > bound) {
            self.errors
                .add(field, format!("must be less than or equal to {bound}"));
        }
    }

    /// Whether no errors were added.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// `Ok(())` or the collected errors.
    pub fn finish(self) -> Result<(), Errors> {
        self.errors.into_result()
    }
}

/// Ecto's integer cast: integers, or strings that parse completely.
pub fn cast_integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) if text.len() < 32 => text.parse().ok(),
        _ => None,
    }
}

/// `"has already been taken"`.
pub const TAKEN: &str = "has already been taken";
/// `"does not exist"`.
pub const DOES_NOT_EXIST: &str = "does not exist";

/// `String.trim/1` on an optional string, keeping `None`.
pub fn trim(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn casts_like_ecto() {
        let params = json!({"name": "  ", "count": "12", "flag": "0", "bad": "x", "when": "2026-01-02T03:04"});
        let mut cs = Changeset::new(&params);
        assert_eq!(cs.string("name"), Change::Set(None));
        assert_eq!(cs.string("missing"), Change::Unchanged);
        assert_eq!(cs.integer("count"), Change::Set(Some(12)));
        assert_eq!(cs.boolean("flag"), Change::Set(Some(false)));
        assert_eq!(cs.integer("bad"), Change::Unchanged);
        assert_eq!(
            cs.datetime("when").or(None).unwrap().to_string(),
            "2026-01-02T03:04:00Z"
        );
        assert_eq!(cs.errors.messages("bad"), ["is invalid"]);
    }
}
