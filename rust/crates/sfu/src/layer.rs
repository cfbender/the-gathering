//! Simulcast layer names, as typed values.

use std::fmt;

/// One of the three simulcast layers a seat's camera publishes, lowest resolution first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// `l`: a quarter of the camera's resolution (rail tiles).
    Low,
    /// `m`: half the resolution (grid cells). New subscriptions start here.
    Medium,
    /// `h`: the full resolution (the pinned board).
    High,
}

impl Layer {
    /// The layer's RTP stream id (`a=rid`).
    pub fn rid(self) -> &'static str {
        match self {
            Self::Low => "l",
            Self::Medium => "m",
            Self::High => "h",
        }
    }

    /// The layer named by `rid`, if it is one of `l`, `m`, `h`.
    pub fn from_rid(rid: &str) -> Option<Self> {
        match rid {
            "l" => Some(Self::Low),
            "m" => Some(Self::Medium),
            "h" => Some(Self::High),
            _ => None,
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.rid())
    }
}

/// Which of a publisher's encodings a packet belongs to: one simulcast layer, or the only
/// stream of a publisher without simulcast (`:single` in the Elixir version).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Encoding {
    Single,
    Layer(Layer),
}

impl Encoding {
    /// The rid to address the publisher's encoding with, for keyframe requests.
    pub(crate) fn rid(self) -> Option<&'static str> {
        match self {
            Self::Single => None,
            Self::Layer(layer) => Some(layer.rid()),
        }
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single => f.write_str("single"),
            Self::Layer(layer) => layer.fmt(f),
        }
    }
}

impl From<Layer> for Encoding {
    fn from(layer: Layer) -> Self {
        Self::Layer(layer)
    }
}
