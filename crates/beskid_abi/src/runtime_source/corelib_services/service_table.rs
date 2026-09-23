use super::super::sources::{
    CANONICAL_CORELIB_ARGS_SOURCE_PATH, CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH, CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH,
    CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH, CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH,
    CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH, CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    CANONICAL_CORELIB_FS_SOURCE_PATH, CANONICAL_CORELIB_HUB_SOURCE_PATH, CANONICAL_CORELIB_MUTEX_SOURCE_PATH,
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH,
    CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    CANONICAL_FOUNDATION_BYTES_SLICE_SOURCE_PATH, CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH,
    CANONICAL_FOUNDATION_ERROR_SOURCE_PATH, CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH,
    CANONICAL_FOUNDATION_PATH_SOURCE_PATH, CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
    CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH, CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH,
    CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH, CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH,
    CANONICAL_FOUNDATION_TIME_SOURCE_PATH, CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
};

/// One ABI-facing service used by a compiler-owned Corelib source unit.
///
/// `name`, `symbol`, and `source_path` are `&'static str` borrowed from the
/// compile-time [`CORELIB_SERVICES`] table, so they cannot round-trip through
/// `serde_json` directly. Manual [`Serialize`]/[`Deserialize`] implementations
/// emit the three fields as owned strings and recover the canonical `&'static
/// str` triple by matching against [`CORELIB_SERVICES`], failing closed with a
/// serde error when no entry matches (a tampered or unknown service).
///
/// The recovery here is a composite 3-field match — no single field uniquely
/// identifies a service (e.g. `__panic_str` / `panic_str` appears under three
/// distinct `source_path` values), so the single-string
/// [`crate::serde_support::recover_static_str`] helper does not fit. The
/// fail-closed contract is the same as the helper's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorelibService {
    pub name: &'static str,
    pub symbol: &'static str,
    pub source_path: &'static str,
}

impl serde::Serialize for CorelibService {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CorelibService", 3)?;
        state.serialize_field("name", self.name)?;
        state.serialize_field("symbol", self.symbol)?;
        state.serialize_field("source_path", self.source_path)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for CorelibService {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Fields {
            name: String,
            symbol: String,
            source_path: String,
        }

        let fields = Fields::deserialize(deserializer)?;
        for service in CORELIB_SERVICES {
            if service.name == fields.name
                && service.symbol == fields.symbol
                && service.source_path == fields.source_path
            {
                return Ok(*service);
            }
        }
        Err(serde::de::Error::custom(format!(
            "unknown CorelibService `{}` / `{}` / `{}`",
            fields.name, fields.symbol, fields.source_path
        )))
    }
}

pub(super) const CORELIB_SERVICES: &[CorelibService] = &[
    CorelibService {
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_open",
        symbol: "beskid_rt_v5_network_open",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_accept",
        symbol: "beskid_rt_v5_network_accept",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_close",
        symbol: "beskid_rt_v5_network_close",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_read",
        symbol: "beskid_rt_v5_network_read",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_write",
        symbol: "beskid_rt_v5_network_write",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_address",
        symbol: "beskid_rt_v5_network_address",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_options",
        symbol: "beskid_rt_v5_network_options",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_set_options",
        symbol: "beskid_rt_v5_network_set_options",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_shutdown_write",
        symbol: "beskid_rt_v5_network_shutdown_write",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_udp_connect",
        symbol: "beskid_rt_v5_network_udp_connect",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_receive",
        symbol: "beskid_rt_v5_network_receive",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_send",
        symbol: "beskid_rt_v5_network_send",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_dns_resolve",
        symbol: "beskid_rt_v5_network_dns_resolve",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_dns_count",
        symbol: "beskid_rt_v5_network_dns_count",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_dns_address",
        symbol: "beskid_rt_v5_network_dns_address",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__network_dns_release",
        symbol: "beskid_rt_v5_network_dns_release",
        source_path: CANONICAL_NETWORK_INTERNAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_write",
        symbol: "syscall_write",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_read",
        symbol: "syscall_read",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_write_bytes",
        symbol: "syscall_write_bytes",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__syscall_read_bytes",
        symbol: "syscall_read_bytes",
        source_path: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    },
    CorelibService {
        name: "__args_count",
        symbol: "beskid_rt_v5_args_count",
        source_path: CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    },
    CorelibService {
        name: "__args_get",
        symbol: "beskid_rt_v5_args_get",
        source_path: CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_yield",
        symbol: "beskid_rt_v5_fiber_yield",
        source_path: CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_now_millis",
        symbol: "fiber_now_millis",
        source_path: CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_processor_count",
        symbol: "fiber_processor_count",
        source_path: CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH,
    },
    CorelibService { name: "__fiber_cancel", symbol: "fiber_cancel", source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH },
    CorelibService { name: "__fiber_detach", symbol: "fiber_detach", source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH },
    CorelibService {
        name: "__fiber_join_detail",
        symbol: "fiber_join_detail",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_join_message",
        symbol: "fiber_join_message",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_join_error_finish",
        symbol: "fiber_join_error_finish",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_join_status",
        symbol: "fiber_join_status",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_join_value",
        symbol: "fiber_join_value",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__tty_winsize",
        symbol: "tty_winsize",
        source_path: CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH,
    },
    CorelibService {
        name: "__tty_winsize",
        symbol: "tty_winsize",
        source_path: CANONICAL_CORELIB_CONSOLE_MACOS_SOURCE_PATH,
    },
    CorelibService {
        name: "__tty_winsize",
        symbol: "tty_winsize",
        source_path: CANONICAL_CORELIB_CONSOLE_WINDOWS_SOURCE_PATH,
    },
    CorelibService {
        name: "__env_get",
        symbol: "env_get",
        source_path: CANONICAL_CORELIB_CONSOLE_TERMINAL_SOURCE_PATH,
    },
    CorelibService {
        name: "__fs_read_text",
        symbol: "beskid_rt_v5_fs_read_text",
        source_path: CANONICAL_CORELIB_FS_SOURCE_PATH,
    },
    CorelibService {
        name: "__fs_write_text",
        symbol: "beskid_rt_v5_fs_write_text",
        source_path: CANONICAL_CORELIB_FS_SOURCE_PATH,
    },
    CorelibService {
        name: "__fs_exists",
        symbol: "beskid_rt_v5_fs_exists",
        source_path: CANONICAL_CORELIB_FS_SOURCE_PATH,
    },
    CorelibService {
        name: "__fs_mkdir",
        symbol: "beskid_rt_v5_fs_mkdir",
        source_path: CANONICAL_CORELIB_FS_SOURCE_PATH,
    },
    CorelibService {
        name: "__fs_delete",
        symbol: "beskid_rt_v5_fs_delete",
        source_path: CANONICAL_CORELIB_FS_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_close",
        symbol: "channel_close",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_create",
        symbol: "channel_create",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_receive_status",
        symbol: "channel_receive_status",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_receive_value",
        symbol: "channel_receive_value",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_send",
        symbol: "channel_send",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_try_receive",
        symbol: "channel_try_receive",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService {
        name: "__channel_try_send",
        symbol: "channel_try_send",
        source_path: CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    },
    CorelibService { name: "__mutex_create", symbol: "mutex_create", source_path: CANONICAL_CORELIB_MUTEX_SOURCE_PATH },
    CorelibService { name: "__mutex_lock", symbol: "mutex_lock", source_path: CANONICAL_CORELIB_MUTEX_SOURCE_PATH },
    CorelibService {
        name: "__mutex_try_lock",
        symbol: "mutex_try_lock",
        source_path: CANONICAL_CORELIB_MUTEX_SOURCE_PATH,
    },
    CorelibService { name: "__mutex_unlock", symbol: "mutex_unlock", source_path: CANONICAL_CORELIB_MUTEX_SOURCE_PATH },
    CorelibService { name: "__hub_create", symbol: "hub_create", source_path: CANONICAL_CORELIB_HUB_SOURCE_PATH },
    CorelibService { name: "__hub_register", symbol: "hub_register", source_path: CANONICAL_CORELIB_HUB_SOURCE_PATH },
    CorelibService {
        name: "__hub_unregister",
        symbol: "hub_unregister",
        source_path: CANONICAL_CORELIB_HUB_SOURCE_PATH,
    },
    CorelibService {
        name: "__hub_wait_receive_status",
        symbol: "hub_wait_receive_status",
        source_path: CANONICAL_CORELIB_HUB_SOURCE_PATH,
    },
    CorelibService {
        name: "__hub_wait_receive_index",
        symbol: "hub_wait_receive_index",
        source_path: CANONICAL_CORELIB_HUB_SOURCE_PATH,
    },
    CorelibService {
        name: "__hub_wait_receive_value",
        symbol: "hub_wait_receive_value",
        source_path: CANONICAL_CORELIB_HUB_SOURCE_PATH,
    },
    CorelibService {
        name: "__wait_group_add",
        symbol: "wait_group_add",
        source_path: CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH,
    },
    CorelibService {
        name: "__wait_group_create",
        symbol: "wait_group_create",
        source_path: CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH,
    },
    CorelibService {
        name: "__wait_group_done",
        symbol: "wait_group_done",
        source_path: CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH,
    },
    CorelibService {
        name: "__wait_group_wait",
        symbol: "wait_group_wait",
        source_path: CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH,
    },
    CorelibService { name: "__array_len", symbol: "array_len", source_path: CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH },
    CorelibService { name: "__env_get", symbol: "env_get", source_path: CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH },
    CorelibService { name: "__env_set", symbol: "env_set", source_path: CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH },
    CorelibService {
        name: "__env_getcwd",
        symbol: "env_getcwd",
        source_path: CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH,
    },
    CorelibService { name: "__str_slice", symbol: "str_slice", source_path: CANONICAL_FOUNDATION_PATH_SOURCE_PATH },
    CorelibService {
        name: "__process_exit",
        symbol: "process_exit",
        source_path: CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
    },
    CorelibService {
        name: "__process_getpid",
        symbol: "process_getpid",
        source_path: CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
    },
    CorelibService {
        name: "__clock_monotonic_nanos",
        symbol: "clock_monotonic_nanos",
        source_path: CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH,
    },
    CorelibService { name: "__str_len", symbol: "str_len", source_path: CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH },
    CorelibService {
        name: "__str_slice",
        symbol: "str_slice",
        source_path: CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH,
    },
    CorelibService {
        name: "__str_slice",
        symbol: "str_slice",
        source_path: CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH,
    },
    CorelibService {
        name: "__str_from_bytes_utf8",
        symbol: "str_from_bytes_utf8",
        source_path: CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH,
    },
    CorelibService {
        name: "__str_slice",
        symbol: "str_slice",
        source_path: CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH,
    },
    CorelibService {
        name: "__clock_realtime_nanos",
        symbol: "clock_realtime_nanos",
        source_path: CANONICAL_FOUNDATION_TIME_SOURCE_PATH,
    },
    CorelibService {
        name: "__clock_monotonic_nanos",
        symbol: "clock_monotonic_nanos",
        source_path: CANONICAL_FOUNDATION_TIME_SOURCE_PATH,
    },
    CorelibService {
        name: "__timer_sleep_until",
        symbol: "beskid_rt_v5_external_sleep_until",
        source_path: CANONICAL_FOUNDATION_TIME_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_FOUNDATION_TIME_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    },
    CorelibService { name: "__gc_collect", symbol: "gc_collect", source_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH },
    CorelibService {
        name: "__panic",
        symbol: "beskid_trap_code",
        source_path: CANONICAL_FOUNDATION_BYTES_SLICE_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH,
    },
    CorelibService {
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_FOUNDATION_ERROR_SOURCE_PATH,
    },
];

/// One traced native value adapter owned by one exact source service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorelibServiceValueDispatch {
    pub symbol: &'static str,
}

/// Return the type-directed value adapters for a canonical source-authorized service.
pub fn canonical_corelib_service_value_dispatch(service: CorelibService) -> Option<CorelibServiceValueDispatch> {
    if service.name == "__fiber_join_value"
        && service.symbol == "fiber_join_value"
        && service.source_path == CANONICAL_CORELIB_FIBER_SOURCE_PATH
    {
        return Some(CorelibServiceValueDispatch { symbol: "fiber_join_value" });
    }
    ((service.name == "__channel_receive_value"
        && service.symbol == "channel_receive_value"
        && service.source_path == CANONICAL_CORELIB_CHANNEL_SOURCE_PATH)
        || (service.name == "__hub_wait_receive_value"
            && service.symbol == "hub_wait_receive_value"
            && service.source_path == CANONICAL_CORELIB_HUB_SOURCE_PATH))
        .then_some(CorelibServiceValueDispatch { symbol: service.symbol })
}
