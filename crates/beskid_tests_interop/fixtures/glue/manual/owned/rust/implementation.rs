//! Ordinary Rust implementation; only the generated producer supplies checked C exports.
pub const LIMIT: usize = 16 * 1024 * 1024;
macro_rules! identity {
    ($name:ident,$ty:ty) => {
        pub fn $name(value: $ty) -> $ty {
            value
        }
    };
}
identity!(glue_i8, i8);
identity!(glue_i16, i16);
identity!(glue_i32, i32);
identity!(glue_i64, i64);
identity!(glue_u8, u8);
identity!(glue_u16, u16);
identity!(glue_u32, u32);
identity!(glue_u64, u64);
identity!(glue_f32, f32);
identity!(glue_f64, f64);
identity!(glue_bool, bool);
identity!(glue_char, char);
identity!(glue_native_width, usize);
pub fn glue_unit() {}
pub fn glue_bytes(value: &[u8]) -> Result<Vec<u8>, i32> {
    if value.len() > LIMIT {
        return Err(1);
    }
    let mut result = Vec::new();
    result.try_reserve_exact(value.len()).map_err(|_| 6)?;
    result.extend_from_slice(value);
    Ok(result)
}
pub fn glue_utf8(value: &str) -> Result<String, i32> {
    if value.len() > LIMIT {
        return Err(1);
    }
    let mut result = String::new();
    result.try_reserve_exact(value.len()).map_err(|_| 6)?;
    result.push_str(value);
    Ok(result)
}
/// Mode 0 succeeds, 1 returns the declared error 6, 2 panics (contained by the generated wrapper)
/// and any other mode aborts the hosting process.
pub fn glue_failure(mode: u32) -> Result<i32, i32> {
    match mode {
        0 => Ok(0),
        1 => Err(6),
        2 => panic!("retained Rust panic fixture"),
        _ => std::process::abort(),
    }
}
pub fn glue_f32_bits(value: f32) -> u32 {
    value.to_bits()
}
pub fn glue_f64_bits(value: f64) -> u64 {
    value.to_bits()
}
pub fn glue_f32_from_bits(value: u32) -> f32 {
    f32::from_bits(value)
}
pub fn glue_f64_from_bits(value: u64) -> f64 {
    f64::from_bits(value)
}
pub fn glue_utf8_exact(value: &str) -> Result<i32, i32> {
    if value.as_bytes() == b"A\0\xf0\x9f\x98\x80" {
        Ok(0)
    } else {
        Err(1)
    }
}
pub fn glue_bytes_exact(value: &[u8]) -> Result<i32, i32> {
    if value == [0, 128, 255, 65] {
        Ok(0)
    } else {
        Err(1)
    }
}
/// Box allocation, ownership publication and destruction belong to the generated bridge.
pub struct ManualOwned {
    pub bytes: Vec<u8>,
}
pub fn make_manual_owned(length: usize) -> Result<ManualOwned, i32> {
    if length > LIMIT {
        return Err(1);
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| 6)?;
    bytes.resize(length, 0);
    Ok(ManualOwned { bytes })
}
/// Length that holds the borrow open until the harness opens its gate. The
/// harness attempts release from another thread while this borrow is live.
pub const BORROW_GATE_LENGTH: usize = 4093;
fn borrow_gate() -> Result<(), i32> {
    let Some(gate) = std::env::var_os("BESKID_V06_GLUE_BORROW_GATE") else {
        return Err(5);
    };
    let gate = std::path::PathBuf::from(gate);
    std::fs::write(gate.with_extension("entered"), b"").map_err(|_| 5)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !gate.exists() {
        if std::time::Instant::now() >= deadline {
            return Err(5);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}
/// Length whose borrow releases the borrowed handle on the same thread, through the consumer
/// release entry the harness names in `<gate>.reentrant` as `<entry address hex>:<token hex>`.
/// Each release must report busy (4); another status `s` returns `Err(100 + s)`.
pub const REENTRANT_RELEASE_LENGTH: usize = 4091;
fn reentrant_release() -> Result<(), i32> {
    let Some(gate) = std::env::var_os("BESKID_V06_GLUE_BORROW_GATE") else {
        return Err(5);
    };
    let request = std::fs::read_to_string(std::path::PathBuf::from(gate).with_extension("reentrant")).map_err(|_| 5)?;
    let (address, token) = request.trim().split_once(':').ok_or(5)?;
    let address = usize::from_str_radix(address, 16).map_err(|_| 5)?;
    let token = u64::from_str_radix(token, 16).map_err(|_| 5)?;
    if address == 0 || token == 0 {
        return Err(5);
    }
    // SAFETY: the harness names the issued consumer opaque release entry of this process image.
    let release = unsafe { std::mem::transmute::<usize, unsafe extern "C" fn(u64) -> i32>(address) };
    for _ in 0..2 {
        let status = unsafe { release(token) };
        if status != 4 {
            return Err(100 + status);
        }
    }
    Ok(())
}
pub fn check_manual_owned(value: &ManualOwned) -> Result<usize, i32> {
    if value.bytes.len() > LIMIT {
        return Err(1);
    }
    if value.bytes.len() == BORROW_GATE_LENGTH {
        borrow_gate()?;
    }
    if value.bytes.len() == REENTRANT_RELEASE_LENGTH {
        reentrant_release()?;
    }
    Ok(value.bytes.len())
}
