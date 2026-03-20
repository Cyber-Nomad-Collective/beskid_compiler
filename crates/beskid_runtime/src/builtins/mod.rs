mod alloc;
mod arrays;
mod events;
mod gc_roots;
mod panic_io;
mod strings;
mod test_helpers;
mod version;
#[cfg(feature = "sched")]
mod sched;
#[cfg(feature = "metrics")]
mod metrics;

pub use alloc::alloc;
pub use arrays::array_new;
pub use events::{EventState, event_get_handler, event_len, event_subscribe, event_unsubscribe_first};
pub use gc_roots::{
    gc_register_root, gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier,
};
pub use panic_io::{panic, panic_str, sys_print, sys_println};
pub use strings::{str_concat, str_len, str_new};
pub use version::beskid_runtime_abi_version;
pub use test_helpers::{test_bytes_len, test_bytes_ptr};
#[cfg(feature = "sched")]
pub use sched::{rt_now_millis, rt_yield};
#[cfg(feature = "metrics")]
pub use metrics::*;
