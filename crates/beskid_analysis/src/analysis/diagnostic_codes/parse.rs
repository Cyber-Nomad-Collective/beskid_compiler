//! Parse errors, `E0101`–`E0199`.
//!
//! Owned by the parse-wording work (0.6.7 slice S2): add one [`CodeInfo`] per parse error below,
//! in code order. Parse codes carry no tags.

use super::{CodeFamily, CodeInfo};

pub const FAMILY: CodeFamily = CodeFamily { name: "parse", first: "E0101", last: "E0199", codes: CODES };

pub const CODES: &[CodeInfo] = &[];
