//! Timestamp type and parsing.
//!
//! Chrona uses `i64` nanoseconds since the Unix epoch, UTC, with no leap-second
//! adjustment. This matches Arrow / Parquet conventions and covers roughly
//! 1677 CE – 2262 CE.
//!
//! On disk, timestamps are encoded as big-endian bytes with the sign bit
//! flipped so that lexicographic byte order matches numeric order across
//! negative and positive timestamps. See [`Ts::to_sortable_bytes`] and
//! [`Ts::from_sortable_bytes`].

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime, Time, UtcOffset};

/// A point in time: nanoseconds since the Unix epoch, UTC.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Ts(pub i64);

impl Ts {
    /// The minimum representable timestamp.
    pub const MIN: Self = Self(i64::MIN);
    /// The maximum representable timestamp.
    pub const MAX: Self = Self(i64::MAX);
    /// The zero point: Unix epoch.
    pub const EPOCH: Self = Self(0);

    /// Construct from raw nanoseconds since epoch.
    #[inline]
    pub const fn from_nanos(ns: i64) -> Self {
        Self(ns)
    }

    /// Raw nanoseconds since epoch.
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Current wall-clock time in UTC.
    ///
    /// Panics if the system clock is before the Unix epoch (essentially never
    /// on real systems).
    pub fn now() -> Self {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch");
        Self(d.as_nanos() as i64)
    }

    /// Parse from an RFC 3339 timestamp (`2026-03-01T12:00:00Z`) or a plain
    /// ISO date (`2026-03-01`, interpreted as UTC midnight).
    pub fn parse(input: &str) -> Result<Self> {
        let s = input.trim();

        // Try RFC 3339 first (full timestamp with timezone).
        if let Ok(odt) = OffsetDateTime::parse(s, &Rfc3339) {
            return Ok(Self::from_odt(odt));
        }

        // Try date-only (YYYY-MM-DD), interpret as UTC midnight.
        if s.len() == 10 && s.as_bytes().get(4) == Some(&b'-') {
            let format = time::macros::format_description!("[year]-[month]-[day]");
            if let Ok(date) = Date::parse(s, format) {
                let odt = date.with_time(Time::MIDNIGHT).assume_offset(UtcOffset::UTC);
                return Ok(Self::from_odt(odt));
            }
        }

        Err(Error::Query(format!(
            "cannot parse timestamp {:?}; expected RFC 3339 or YYYY-MM-DD",
            input
        )))
    }

    /// Format as an RFC 3339 timestamp.
    pub fn to_rfc3339(self) -> String {
        match OffsetDateTime::from_unix_timestamp_nanos(self.0 as i128) {
            Ok(odt) => odt
                .format(&Rfc3339)
                .unwrap_or_else(|_| format!("ns={}", self.0)),
            Err(_) => format!("ns={}", self.0),
        }
    }

    fn from_odt(odt: OffsetDateTime) -> Self {
        let ns = odt.unix_timestamp_nanos();
        // Clamp to i64 range; realistically this only matters for far-future
        // dates parsed from user input.
        let clamped = ns.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        Self(clamped)
    }

    /// Encode as 8 big-endian bytes with the sign bit flipped. The resulting
    /// byte string sorts in numeric order under lexicographic comparison
    /// across the entire `i64` range.
    #[inline]
    pub fn to_sortable_bytes(self) -> [u8; 8] {
        let flipped = (self.0 as u64) ^ 0x8000_0000_0000_0000;
        flipped.to_be_bytes()
    }

    /// Inverse of [`Ts::to_sortable_bytes`].
    #[inline]
    pub fn from_sortable_bytes(b: [u8; 8]) -> Self {
        let raw = u64::from_be_bytes(b) ^ 0x8000_0000_0000_0000;
        Self(raw as i64)
    }
}

impl fmt::Display for Ts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_roundtrip() {
        let ts = Ts::EPOCH;
        let bytes = ts.to_sortable_bytes();
        assert_eq!(bytes, [0x80, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(Ts::from_sortable_bytes(bytes), ts);
    }

    #[test]
    fn min_sorts_before_max() {
        assert!(Ts::MIN.to_sortable_bytes() < Ts::MAX.to_sortable_bytes());
    }

    #[test]
    fn negative_sorts_before_positive() {
        let neg = Ts(-1_000);
        let pos = Ts(1_000);
        assert!(neg.to_sortable_bytes() < pos.to_sortable_bytes());
    }

    #[test]
    fn parse_rfc3339() {
        let ts = Ts::parse("2026-03-01T00:00:00Z").unwrap();
        assert!(ts.0 > 0);
    }

    #[test]
    fn parse_date_only() {
        let a = Ts::parse("2026-03-01").unwrap();
        let b = Ts::parse("2026-03-01T00:00:00Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn parse_invalid() {
        assert!(Ts::parse("not a date").is_err());
        assert!(Ts::parse("2026/03/01").is_err());
    }

    #[test]
    fn display_roundtrip() {
        let ts = Ts::parse("2026-03-01T12:34:56Z").unwrap();
        assert_eq!(ts.to_rfc3339(), "2026-03-01T12:34:56Z");
    }
}
