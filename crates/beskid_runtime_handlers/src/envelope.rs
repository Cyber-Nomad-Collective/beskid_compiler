//! Dispatch envelope payload decoding for language handler wrappers.
//!
//! Layout matches v3 [`RuntimeInteropEnvelope`](https://beskid-lang.org/platform-spec/language-meta/interop/interop-contracts/adr/0004-dispatch-envelope-layout/):
//! 16-byte header, then 8-byte payload slots per parameter.

use beskid_abi::{BeskidStr, DISPATCH_ENVELOPE_HEADER_SIZE};

#[inline]
fn payload_offset(param_index: usize) -> usize {
    DISPATCH_ENVELOPE_HEADER_SIZE as usize + param_index * 8
}

/// Load a raw payload slot (pointer-sized word) from the envelope.
#[inline]
pub fn load_raw<T: Copy>(enum_ptr: *const u8, param_index: usize) -> T {
    unsafe { *(enum_ptr.add(payload_offset(param_index)) as *const T) }
}

/// Load a `ptr` dispatch parameter (handle stored as pointer-sized word).
#[inline]
pub fn load_ptr(enum_ptr: *const u8, param_index: usize) -> *const u8 {
    load_raw::<*const u8>(enum_ptr, param_index)
}

/// Load a `string` dispatch parameter.
#[inline]
pub fn load_string(enum_ptr: *const u8, param_index: usize) -> *const BeskidStr {
    load_raw::<*const BeskidStr>(enum_ptr, param_index)
}

/// Load a `u64` dispatch parameter.
#[inline]
pub fn load_u64(enum_ptr: *const u8, param_index: usize) -> u64 {
    load_raw::<u64>(enum_ptr, param_index)
}

/// Load a `usize` dispatch parameter.
#[inline]
pub fn load_usize(enum_ptr: *const u8, param_index: usize) -> usize {
    load_raw::<usize>(enum_ptr, param_index)
}

/// Load an `i64` dispatch parameter.
#[inline]
pub fn load_i64(enum_ptr: *const u8, param_index: usize) -> i64 {
    load_raw::<i64>(enum_ptr, param_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_abi::{BeskidArray, TAG_SYSCALL_WRITE};

    #[repr(C)]
    struct RuntimeInteropEnvelope {
        type_desc_ptr: *const u8,
        tag: i32,
        pad: i32,
    }

    #[repr(C)]
    struct SyscallWriteEnvelope {
        header: RuntimeInteropEnvelope,
        fd: u64,
        text: *const BeskidStr,
    }

    #[repr(C)]
    struct BytesGetEnvelope {
        header: RuntimeInteropEnvelope,
        array: *const BeskidArray,
        index: u64,
    }

    #[test]
    fn envelope_header_size_matches_abi() {
        assert_eq!(DISPATCH_ENVELOPE_HEADER_SIZE, 16);
        assert_eq!(payload_offset(0), 16);
        assert_eq!(payload_offset(1), 24);
    }

    #[test]
    fn syscall_write_payload_offsets() {
        let text = std::ptr::null::<BeskidStr>();
        let envelope = SyscallWriteEnvelope {
            header: RuntimeInteropEnvelope { type_desc_ptr: std::ptr::null(), tag: TAG_SYSCALL_WRITE, pad: 0 },
            fd: 1,
            text,
        };
        let enum_ptr = &envelope as *const SyscallWriteEnvelope as *const u8;
        assert_eq!(load_u64(enum_ptr, 0), 1);
        assert_eq!(load_string(enum_ptr, 1), text);
    }

    #[test]
    fn bytes_get_payload_offsets() {
        let array = std::ptr::null::<BeskidArray>();
        let envelope = BytesGetEnvelope {
            header: RuntimeInteropEnvelope { type_desc_ptr: std::ptr::null(), tag: 1, pad: 0 },
            array,
            index: 3,
        };
        let enum_ptr = &envelope as *const BytesGetEnvelope as *const u8;
        assert_eq!(load_ptr(enum_ptr, 0), array as *const u8);
        assert_eq!(load_u64(enum_ptr, 1), 3);
    }
}
