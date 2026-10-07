//! Identifier newtypes, so a room id is never passed where a peer id belongs.

use std::fmt;
use std::sync::Arc;

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub(crate) struct $name(Arc<str>);

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(Arc::from(value))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

id!(
    /// A webcam table (the channel's room id).
    RoomId
);

id!(
    /// A seat's (or spectator's) connection at a table; a rejoin brings a new one.
    PeerId
);
