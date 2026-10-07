//! A field of a partial update: left out (keep the stored value) or given (possibly null).

use serde::{Deserialize, Deserializer};

/// A partial-update field. Deserialize it with `#[serde(default)]` so a missing key is
/// [`Patch::Unchanged`]; `null` is `Set(None)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Patch<T> {
    /// The key was left out.
    #[default]
    Unchanged,
    /// The key was given; `None` clears the value.
    Set(Option<T>),
}

impl<T> Patch<T> {
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
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Patch<U> {
        match self {
            Self::Unchanged => Patch::Unchanged,
            Self::Set(value) => Patch::Set(value.map(f)),
        }
    }
}

impl Patch<String> {
    /// Trims the value; a blank string clears it (forms submit `""` for an emptied field).
    #[must_use]
    pub fn trimmed(self) -> Self {
        match self {
            Self::Set(Some(value)) => {
                let value = value.trim();
                Self::Set((!value.is_empty()).then(|| value.to_owned()))
            }
            other => other,
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(Self::Set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Update {
        #[serde(default)]
        name: Patch<String>,
        #[serde(default)]
        count: Patch<i64>,
    }

    #[test]
    fn tells_missing_null_and_values_apart() {
        let update: Update = serde_json::from_str(r#"{"name": null}"#).unwrap();
        assert_eq!(update.name, Patch::Set(None));
        assert_eq!(update.count, Patch::Unchanged);
        let update: Update = serde_json::from_str(r#"{"name": "  Drew ", "count": 3}"#).unwrap();
        assert_eq!(update.name.trimmed(), Patch::Set(Some("Drew".into())));
        assert_eq!(update.count.or(Some(1)), Some(3));
        let update: Update = serde_json::from_str(r#"{"name": "   "}"#).unwrap();
        assert_eq!(update.name.trimmed(), Patch::Set(None));
        assert!(serde_json::from_str::<Update>(r#"{"count": "3"}"#).is_err());
    }
}
