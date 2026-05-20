//! Cooperative M:N scheduler (Phase A: single GC mutator; fibers use [`corosensei`] stacks).

mod run_loop;
mod spawn;
mod state;
mod syscall_pool;
mod tls;

pub use run_loop::{fiber_join, fiber_yield, park_current, run_closure_as_main, run_main_fiber};
pub use spawn::{fiber_cancel, fiber_detach, fiber_spawn};
pub use state::FiberKey;
pub use syscall_pool::{run_blocking, run_blocking_value};
pub use tls::{
    current_fiber_cancelled, current_fiber_id, current_fiber_key, fiber_now_millis,
    in_fiber_scheduler, init, processor_count, wake_fiber,
};
