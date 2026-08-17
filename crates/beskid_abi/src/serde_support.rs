//! Shared serde helpers for `&'static str` round-trip recovery.
//!
//! Types that borrow canonical `&'static str` from compile-time tables (e.g. builtin symbol
//! tables, Corelib service registries) cannot round-trip through `serde_json` directly:
//! deserialization produces an owned `String` whose lifetime is not `'static`, and the borrow
//! cannot be promoted. The helpers in this module encapsulate the fail-closed recovery pattern —
//! read an owned string, look it up against a static table, return the canonical `&'static str`
//! or a serde `custom` error when no entry matches (a tampered or unknown value).
//!
//! Composite identities (multiple `&'static str` fields that only identify a canonical entry when
//! matched together) do not fit the single-string helper here; their owners keep a struct-level
//! composite match but follow the same fail-closed contract.

use serde::Deserializer;

/// Recover a canonical `&'static str` from a deserialized owned `String`.
///
/// Reads a `String` from `deserializer`, then calls `lookup` with the borrowed `&str`. When
/// `lookup` returns `Some(canonical)`, that canonical `&'static str` is returned; otherwise the
/// helper fails closed with a `serde::de::Error::custom` message of the form
/// `unknown <what> \`<value>\``.
///
/// This encapsulates the `&'static str` round-trip problem: the deserialized `String` cannot be
/// promoted to `'static`, so the canonical borrow must be recovered from a compile-time table
/// supplied by the caller. The fail-closed contract means a tampered or unknown value can never
/// silently produce a non-canonical `&'static str`.
pub fn recover_static_str<'de, D, F>(deserializer: D, what: &'static str, lookup: F) -> Result<&'static str, D::Error>
where
    D: Deserializer<'de>,
    F: Fn(&str) -> Option<&'static str>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    match lookup(&value) {
        Some(canonical) => Ok(canonical),
        None => Err(serde::de::Error::custom(format!("unknown {what} `{value}`"))),
    }
}
