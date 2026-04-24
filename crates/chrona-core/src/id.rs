//! Strongly-typed identifier newtypes for Chrona.
//!
//! All internal identifiers are `u64` (or `u32` for strings) wrapped in a
//! newtype so the compiler catches mix-ups like "I passed an EdgeId where I
//! meant a NodeId."

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident, $inner:ty, $prefix:literal) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
            Serialize, Deserialize,
        )]
        #[repr(transparent)]
        pub struct $name(pub $inner);

        impl $name {
            /// The reserved zero id — MUST NOT be assigned to a real entity.
            pub const ZERO: Self = Self(0);

            /// Return the inner scalar.
            #[inline]
            pub const fn raw(self) -> $inner {
                self.0
            }

            /// Construct from a raw value. Callers are responsible for ensuring
            /// the value is valid (non-zero for real entities).
            #[inline]
            pub const fn from_raw(v: $inner) -> Self {
                Self(v)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }

        impl From<$inner> for $name {
            #[inline]
            fn from(v: $inner) -> Self {
                Self(v)
            }
        }

        impl From<$name> for $inner {
            #[inline]
            fn from(v: $name) -> Self {
                v.0
            }
        }
    };
}

id_type!(
    /// Internal node identifier, assigned monotonically. The zero value is
    /// reserved and must not be used for real nodes.
    NodeId,
    u64,
    "n"
);

id_type!(
    /// Internal edge identifier.
    EdgeId,
    u64,
    "e"
);

id_type!(
    /// Event identifier — monotonically increasing per database file.
    EventId,
    u64,
    "ev"
);

id_type!(
    /// Interned string identifier.
    StringId,
    u32,
    "s"
);
