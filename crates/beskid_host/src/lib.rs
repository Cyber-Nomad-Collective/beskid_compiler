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

#[cfg(test)]
mod tests {
    use beskid_abi::{TAG_WAIT_GROUP_CREATE, TAG_WAIT_GROUP_WAIT};

    fn dispatch_envelope(tag: i32, argument: i64) -> [u8; 24] {
        let mut envelope = [0u8; 24];
        envelope[8..12].copy_from_slice(&tag.to_le_bytes());
        envelope[16..24].copy_from_slice(&argument.to_le_bytes());
        envelope
    }

    #[test]
    fn host_registration_does_not_override_wait_group_dispatch() {
        assert_eq!(super::beskid_host_register_all(), 0);

        let create = dispatch_envelope(TAG_WAIT_GROUP_CREATE, 0);
        let group = beskid_runtime::interop_dispatch_i64(create.as_ptr());
        assert!(group > 0, "wait-group create must reach the runtime handler");

        let wait = dispatch_envelope(TAG_WAIT_GROUP_WAIT, group);
        assert_eq!(
            beskid_runtime::interop_dispatch_i64(wait.as_ptr()),
            beskid_runtime::status::STATUS_OK,
            "wait-group wait must reach the runtime handler"
        );
    }
}
