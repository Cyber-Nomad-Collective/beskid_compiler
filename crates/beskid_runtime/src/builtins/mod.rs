mod alloc;
mod arrays;
mod events;
mod gc_roots;
#[cfg(feature = "metrics")]
mod metrics;
mod panic_io;
#[cfg(feature = "sched")]
mod sched;
mod strings;
mod test_helpers;
mod version;

pub use alloc::alloc;
pub use arrays::array_new;
pub use events::{
    EventState, event_get_handler, event_len, event_subscribe, event_unsubscribe_first,
};
pub use gc_roots::{
    gc_register_root, gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier,
};
#[cfg(feature = "metrics")]
pub use metrics::*;
pub use panic_io::{panic, panic_str, syscall_write};
#[cfg(feature = "sched")]
pub use sched::{rt_now_millis, rt_yield};
pub use strings::{str_concat, str_len, str_new};
pub use test_helpers::{test_bytes_len, test_bytes_ptr};
pub use version::beskid_runtime_abi_version;
