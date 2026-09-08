use crate::abi_v5::{AbiManifestV5, canonical_runtime_package, canonical_source_hash};
use crate::{AbiParamKind, AbiReturnKind};

use super::capabilities::RuntimeCapabilityError;
use super::sources::{
    CANONICAL_CORELIB_ARGS_SOURCE_PATH, CANONICAL_CORELIB_CHANNEL_SOURCE_PATH,
    CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH,
    CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH, CANONICAL_CORELIB_FIBER_SOURCE_PATH, CANONICAL_CORELIB_FS_SOURCE_PATH,
    CANONICAL_CORELIB_HUB_SOURCE_PATH,
    CANONICAL_CORELIB_MUTEX_SOURCE_PATH, CANONICAL_CORELIB_SYSCALL_SOURCE_PATH,
    CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH, CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH,
    CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH, CANONICAL_FOUNDATION_ERROR_SOURCE_PATH,
    CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH,
    CANONICAL_FOUNDATION_PATH_SOURCE_PATH, CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
    CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH, CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH,
    CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH, CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH,
    CANONICAL_FOUNDATION_TIME_SOURCE_PATH, canonical_corelib_service_sources,
};

/// The canonical compiler-owned source file for one Foundation service unit.
///
/// Authority is tied to this checked-in file identity as well as embedded bytes and logical
/// module path. A user project that copies `Testing/Assert.bd` cannot acquire it.
///
/// The returned path is lexically normalized so `Path::starts_with` / equality against
/// resolved Foundation `source_root` values succeed. Leaving `../..` from
/// `CARGO_MANIFEST_DIR` intact made materialized Corelib deps drop panic/syscall provenance
/// and fall through to Dynamic `__panic_str` (Corelib gate).
pub fn canonical_corelib_service_source_path(logical_path: &str) -> Option<std::path::PathBuf> {
    let (package, relative) = match logical_path {
        CANONICAL_CORELIB_SYSCALL_SOURCE_PATH => ("foundation", "Core/Syscall/Syscall.bd"),
        CANONICAL_CORELIB_ARGS_SOURCE_PATH => ("foundation", "Core/Args/Args.bd"),
        CANONICAL_CORELIB_CONCURRENCY_SOURCE_PATH => ("concurrency", "Concurrency.bd"),
        CANONICAL_CORELIB_FIBER_SOURCE_PATH => ("concurrency", "Concurrency/Fiber.bd"),
        CANONICAL_CORELIB_CONSOLE_LINUX_SOURCE_PATH => ("console", "Platform/Linux.bd"),
        CANONICAL_CORELIB_FS_SOURCE_PATH => ("foundation", "Core/FS/FS.bd"),
        CANONICAL_CORELIB_CHANNEL_SOURCE_PATH => ("concurrency", "Concurrency/Channel.bd"),
        CANONICAL_CORELIB_MUTEX_SOURCE_PATH => ("concurrency", "Concurrency/Mutex.bd"),
        CANONICAL_CORELIB_HUB_SOURCE_PATH => ("concurrency", "Concurrency/Hub.bd"),
        CANONICAL_CORELIB_WAIT_GROUP_SOURCE_PATH => ("concurrency", "Concurrency/WaitGroup.bd"),
        CANONICAL_FOUNDATION_ARRAY_SOURCE_PATH => ("foundation", "Core/Collections/Array.bd"),
        CANONICAL_FOUNDATION_ENVIRONMENT_SOURCE_PATH => ("foundation", "Core/Environment/Environment.bd"),
        CANONICAL_FOUNDATION_PATH_SOURCE_PATH => ("foundation", "Core/Path/Path.bd"),
        CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH => ("foundation", "Core/Process/Process.bd"),
        CANONICAL_FOUNDATION_RANDOM_SOURCE_PATH => ("foundation", "Core/Random/Random.bd"),
        CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH => ("foundation", "Core/String/Core.bd"),
        CANONICAL_FOUNDATION_STRING_UTF8_SOURCE_PATH => ("foundation", "Core/String/Utf8.bd"),
        CANONICAL_FOUNDATION_TEXT_CURSOR_SOURCE_PATH => ("foundation", "Core/Text/Cursor.bd"),
        CANONICAL_FOUNDATION_TIME_SOURCE_PATH => ("foundation", "Core/Time/Time.bd"),
        CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH => ("foundation", "Testing/Assert.bd"),
        CANONICAL_FOUNDATION_OUTPUT_SOURCE_PATH => ("foundation", "Core/Output/Output.bd"),
        CANONICAL_FOUNDATION_ERROR_SOURCE_PATH => ("foundation", "Core/Error/Error.bd"),
        _ => return None,
    };
    Some(normalize_lexically(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corelib/packages")
            .join(package)
            .join("src")
            .join(relative),
    ))
}

/// Collapse `.` / `..` components without requiring the path to exist on disk.
fn normalize_lexically(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::new();
    path.components().for_each(|component| match component {
        Component::ParentDir => {
            out.pop();
        }
        Component::CurDir => {}
        other => out.push(other.as_os_str()),
    });
    out
}

/// One source-independent ABI slot selected for a source-authorized Corelib service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorelibServiceAbiType {
    Pointer,
    String,
    Usize,
    I64,
    I32,
    U8,
    F64,
    Void,
    Never,
}

/// The unique native ABI shape behind a source-authorized Corelib service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorelibServiceAbi {
    pub parameters: Vec<CorelibServiceAbiType>,
    pub result: CorelibServiceAbiType,
}

/// Type-directed native value adapters owned by one exact source service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorelibServiceValueDispatch {
    pub scalar_symbol: &'static str,
    pub managed_symbol: &'static str,
}

/// Return the type-directed value adapters for a canonical source-authorized service.
pub fn canonical_corelib_service_value_dispatch(service: CorelibService) -> Option<CorelibServiceValueDispatch> {
    (service.name == "__channel_receive_value"
        && service.symbol == "channel_receive_value"
        && service.source_path == CANONICAL_CORELIB_CHANNEL_SOURCE_PATH)
        .then_some(CorelibServiceValueDispatch {
            scalar_symbol: "channel_receive_value",
            managed_symbol: "channel_receive_ptr",
        })
}

/// Resolve a service adapter against the canonical ABI-v5 bindings.
///
/// Generated Corelib-service bindings and soft builtins intentionally share this one lookup.
/// Conflicting target shapes or duplicate soft-builtin declarations fail closed.
pub fn canonical_corelib_service_abi(service: CorelibService) -> Option<CorelibServiceAbi> {
    canonical_corelib_service_abi_for_adapter(service.symbol)
}

/// Resolve the unique canonical ABI-v5 shape for an already source-authorized adapter symbol.
pub fn canonical_corelib_service_abi_for_adapter(symbol: &str) -> Option<CorelibServiceAbi> {
    let mut bindings = crate::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS
        .iter()
        .filter(|binding| binding.adapter == symbol);
    if let Some(binding) = bindings.next() {
        if bindings.any(|candidate| candidate.params != binding.params || candidate.result != binding.result) {
            return None;
        }
        return Some(CorelibServiceAbi {
            parameters: binding.params.iter().copied().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()?,
            result: corelib_service_abi_type(binding.result)?,
        });
    }

    let mut builtins = crate::all_builtin_specs().filter(|binding| binding.symbol == symbol);
    let binding = builtins.next()?;
    if builtins.any(|candidate| candidate.params != binding.params || candidate.returns != binding.returns) {
        return None;
    }
    Some(CorelibServiceAbi {
        parameters: binding.params.iter().copied().map(corelib_soft_builtin_parameter_type).collect(),
        result: corelib_soft_builtin_return_type(binding.returns),
    })
}

fn corelib_service_abi_type(ty: &str) -> Option<CorelibServiceAbiType> {
    Some(match ty {
        "pointer" => CorelibServiceAbiType::Pointer,
        "string" => CorelibServiceAbiType::String,
        "usize" | "isize" => CorelibServiceAbiType::Usize,
        "i64" => CorelibServiceAbiType::I64,
        "i32" | "u32" => CorelibServiceAbiType::I32,
        "u8" => CorelibServiceAbiType::U8,
        "f64" => CorelibServiceAbiType::F64,
        "void" => CorelibServiceAbiType::Void,
        "never" => CorelibServiceAbiType::Never,
        _ => return None,
    })
}

fn corelib_soft_builtin_parameter_type(ty: AbiParamKind) -> CorelibServiceAbiType {
    match ty {
        AbiParamKind::Ptr => CorelibServiceAbiType::Pointer,
        AbiParamKind::I64 => CorelibServiceAbiType::I64,
        AbiParamKind::F64 => CorelibServiceAbiType::F64,
    }
}

fn corelib_soft_builtin_return_type(ty: AbiReturnKind) -> CorelibServiceAbiType {
    match ty {
        AbiReturnKind::Void => CorelibServiceAbiType::Void,
        AbiReturnKind::Never => CorelibServiceAbiType::Never,
        AbiReturnKind::Ptr => CorelibServiceAbiType::Pointer,
        AbiReturnKind::I64 => CorelibServiceAbiType::I64,
        AbiReturnKind::I32 => CorelibServiceAbiType::I32,
        AbiReturnKind::F64 => CorelibServiceAbiType::F64,
    }
}

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

const CORELIB_SERVICES: &[CorelibService] = &[
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
    CorelibService {
        name: "__fiber_cancel",
        symbol: "fiber_cancel",
        source_path: CANONICAL_CORELIB_FIBER_SOURCE_PATH,
    },
    CorelibService {
        name: "__fiber_detach",
        symbol: "fiber_detach",
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
        name: "__channel_receive",
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
        name: "__hub_wait_receive",
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
        name: "__panic_str",
        symbol: "beskid_trap_message",
        source_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH,
    },
    CorelibService { name: "__gc_collect", symbol: "gc_collect", source_path: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH },
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

/// Compiler-owned proof that a unit belongs to the embedded Corelib service corpus.
///
/// This has a separate type and constructor from [`super::RuntimeIntrinsicCapability`], so Corelib
/// never inherits raw bootstrap intrinsic authority.
#[derive(Debug)]
pub struct CorelibServiceProof {
    source_hash: String,
    source_paths: Vec<String>,
}

impl CorelibServiceProof {
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }

    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.source_paths.iter().any(|candidate| candidate == logical_path)
    }
}

#[derive(Debug)]
pub struct CorelibServiceCapability {
    proof: CorelibServiceProof,
}

impl CorelibServiceCapability {
    pub fn authorizes_source(&self, logical_path: &str) -> bool {
        self.proof.authorizes_source(logical_path)
    }

    pub fn service_for_source(&self, logical_path: &str, name: &str) -> Option<CorelibService> {
        self.authorizes_source(logical_path)
            .then(|| {
                CORELIB_SERVICES
                    .iter()
                    .copied()
                    .find(|service| service.name == name && service.source_path == logical_path)
            })
            .flatten()
    }

    pub fn services(&self) -> &'static [CorelibService] {
        CORELIB_SERVICES
    }
}

/// Mint the distinct Corelib syscall service capability from the compiler-embedded source.
///
/// Callers still have to prove their assembled unit exactly matches this corpus before the
/// capability can be attached to syntax facts. The ABI manifest is validated here to prevent a
/// drifted target contract from being combined with compiler-owned services.
pub fn canonical_corelib_service_capability(
    manifest: &AbiManifestV5,
) -> Result<CorelibServiceCapability, RuntimeCapabilityError> {
    manifest.validate().map_err(|_| RuntimeCapabilityError::InvalidManifest)?;
    if manifest.trusted_runtime_package.as_ref() != Some(&canonical_runtime_package()) {
        return Err(RuntimeCapabilityError::InvalidManifest);
    }
    let sources = canonical_corelib_service_sources();
    let source_hash = canonical_source_hash(&sources).map_err(|_| RuntimeCapabilityError::SourceSetMismatch)?;
    Ok(CorelibServiceCapability {
        proof: CorelibServiceProof {
            source_hash,
            source_paths: sources.into_iter().map(|unit| unit.logical_path).collect(),
        },
    })
}

/// Backwards-compatible spelling for callers that only need the syscall subset.
///
/// The returned capability remains source-scoped; it cannot authorize assertion services for a
/// syscall unit.
pub fn canonical_corelib_syscall_service_capability(
    manifest: &AbiManifestV5,
) -> Result<CorelibServiceCapability, RuntimeCapabilityError> {
    canonical_corelib_service_capability(manifest)
}
