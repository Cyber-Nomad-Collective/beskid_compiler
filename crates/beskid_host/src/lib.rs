//! Host OS policy layer: FS, environment, process, and terminal geometry.
//!
//! Handlers register with [`beskid_runtime`] via [`beskid_host_register_all`] at process start.

mod env;
mod fs;
mod generated;
mod process;
mod strings;
mod tty;

pub use env::{env_get, env_getcwd, env_set};
pub use fs::{fs_delete, fs_exists, fs_mkdir, fs_read_text, fs_write_text};
pub use generated::host_handlers::beskid_host_register_all;
pub use process::{process_exit, process_getpid, process_last_exit_code, process_run};
pub use tty::tty_winsize;
