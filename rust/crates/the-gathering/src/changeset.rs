//! Casts fields out of untyped JSON params: blank and whitespace-only strings become `None`,
//! non-blank strings are kept untrimmed, integers accept numeric strings, booleans accept
//! `"true"`/`"false"`/`"1"`/`"0"`, and anything else fails with `"is invalid"`.
//!
//! Transitional: handlers are moving to typed request bodies, after which this module goes
//! away and only [`Validator`] remains.

use serde_json::{Map, Value};

use std::ops::{Deref, DerefMut};

use crate::db::{IsoDate, UtcDateTime};
use crate::patch::Patch;
use crate::validation::{ValidationError, Validator};

/// `null` or a string that trims to `""`.
fn blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// Casts params and validates the results; cast failures land in the same [`Validator`].
#[derive(Debug, Default)]
pub struct Changeset<'a> {
    params: Option<&'a Map<String, Value>>,
    validator: Validator,
}

impl Deref for Changeset<'_> {
    type Target = Validator;

    fn deref(&self) -> &Validator {
        &self.validator
    }
}

impl DerefMut for Changeset<'_> {
    fn deref_mut(&mut self) -> &mut Validator {
        &mut self.validator
    }
}

impl<'a> Changeset<'a> {
    /// Casts from `params` (a JSON object; anything else casts nothing).
    pub fn new(params: &'a Value) -> Self {
        Self {
            params: params.as_object(),
            validator: Validator::new(),
        }
    }

    /// The raw param.
    pub fn raw(&self, field: &str) -> Option<&'a Value> {
        self.params.and_then(|params| params.get(field))
    }

    fn cast<T>(&mut self, field: &str, convert: impl FnOnce(&Value) -> Option<T>) -> Patch<T> {
        match self.raw(field) {
            None => Patch::Unchanged,
            Some(value) if blank(value) => Patch::Set(None),
            Some(value) => {
                if let Some(cast) = convert(value) {
                    Patch::Set(Some(cast))
                } else {
                    self.errors.add(field, "is invalid");
                    Patch::Unchanged
                }
            }
        }
    }

    /// A `:string` field.
    pub fn string(&mut self, field: &str) -> Patch<String> {
        self.cast(field, |value| value.as_str().map(str::to_owned))
    }

    /// An `:integer` field.
    pub fn integer(&mut self, field: &str) -> Patch<i64> {
        self.cast(field, cast_integer)
    }

    /// A `:boolean` field.
    pub fn boolean(&mut self, field: &str) -> Patch<bool> {
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
    pub fn datetime(&mut self, field: &str) -> Patch<UtcDateTime> {
        self.cast(field, |value| value.as_str().and_then(UtcDateTime::parse))
    }

    /// A `:date` field.
    pub fn date(&mut self, field: &str) -> Patch<IsoDate> {
        self.cast(field, |value| value.as_str().and_then(IsoDate::parse))
    }

    /// `Ok(())` or the collected errors.
    pub fn finish(self) -> Result<(), ValidationError> {
        self.validator.finish()
    }
}

/// Integers, or strings that parse completely as one.
pub fn cast_integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) if text.len() < 32 => text.parse().ok(),
        _ => None,
    }
}

/// Trims an optional string, keeping `None`.
pub fn trim(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn casts_params() {
        let params = json!({"name": "  ", "count": "12", "flag": "0", "bad": "x", "when": "2026-01-02T03:04"});
        let mut cs = Changeset::new(&params);
        assert_eq!(cs.string("name"), Patch::Set(None));
        assert_eq!(cs.string("missing"), Patch::Unchanged);
        assert_eq!(cs.integer("count"), Patch::Set(Some(12)));
        assert_eq!(cs.boolean("flag"), Patch::Set(Some(false)));
        assert_eq!(cs.integer("bad"), Patch::Unchanged);
        assert_eq!(
            cs.datetime("when").or(None).unwrap().to_string(),
            "2026-01-02T03:04:00Z"
        );
        assert_eq!(cs.errors.messages("bad"), ["is invalid"]);
    }
}
