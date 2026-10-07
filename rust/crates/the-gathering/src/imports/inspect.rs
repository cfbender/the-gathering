//! `inspect/1` renderings the Elixir importer put into user-facing messages.

use serde_json::Value;

use crate::error::Errors;

use super::table::inspect_string;

/// `inspect/1` of a decoded JSON value (`nil`, numbers, `"text"`, lists, and maps with
/// string keys).
pub fn value(value: &Value) -> String {
    match value {
        Value::Null => "nil".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => inspect_string(text),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(self::value).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(map) => format!(
            "%{{{}}}",
            map.iter()
                .map(|(key, item)| format!("{} => {}", inspect_string(key), self::value(item)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// An atom-keyed map from `traverse_errors/2` output, as `inspect/1` prints it.
fn atom_map(value: &Value) -> String {
    match value {
        Value::Object(map) => format!(
            "%{{{}}}",
            map.iter()
                .map(|(key, item)| format!("{key}: {}", atom_map(item)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(atom_map).collect::<Vec<_>>().join(", ")
        ),
        other => self::value(other),
    }
}

/// `inspect(Ecto.Changeset.traverse_errors(changeset, ...))`, e.g.
/// `%{name: ["can't be blank"]}`.
pub fn errors(errors: &Errors) -> String {
    atom_map(&errors.to_json())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_like_inspect() {
        assert_eq!(value(&json!(null)), "nil");
        assert_eq!(value(&json!("10")), "\"10\"");
        let mut nested = Errors::single("name", "can't be blank");
        nested.set_nested(
            "seats",
            vec![
                Errors::new(),
                Errors::single("kills", "must be greater than or equal to 0"),
            ],
        );
        assert_eq!(
            errors(&nested),
            "%{name: [\"can't be blank\"], seats: [%{}, %{kills: [\"must be greater than or equal to 0\"]}]}"
        );
    }
}
