//! Language-owned runtime dispatch handlers registered via [`beskid_language_register_all`].
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod bytes;
mod envelope;
mod generated;
mod strings;
mod test_helpers;

pub use bytes::{bytes_compare, bytes_get};
pub use envelope::{load_i64, load_ptr, load_raw, load_string, load_u64, load_usize};
pub use generated::language_handlers::beskid_language_register_all;
pub use strings::str_eq;
pub use test_helpers::{test_bytes_len, test_bytes_ptr};
