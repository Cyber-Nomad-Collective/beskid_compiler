use crate::channel;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_create(capacity: i64, _flags: i64) -> i64 {
    let _ = _flags;
    channel::channel_create(capacity)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_send(channel_id: i64, value: i64) -> i64 {
    channel::channel_send(channel_id, value)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_receive(channel_id: i64, out_value: *mut i64) -> i64 {
    channel::channel_receive(channel_id, out_value)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_receive_status(channel_id: i64) -> i64 {
    channel::channel_receive_status(channel_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_receive_value(channel_id: i64) -> i64 {
    channel::channel_receive_value(channel_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_try_send(channel_id: i64, value: i64) -> i64 {
    channel::channel_try_send(channel_id, value)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_try_receive(channel_id: i64, out_value: *mut i64) -> i64 {
    channel::channel_try_receive(channel_id, out_value)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_close(channel_id: i64) {
    channel::channel_close(channel_id);
}

/// Phase B pointer-payload send; see [`channel::channel_send_ptr`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_send_ptr(channel_id: i64, value_ptr: *mut u8) -> i64 {
    channel::channel_send_ptr(channel_id, value_ptr)
}

/// Phase B pointer-payload try-send; see [`channel::channel_try_send_ptr`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_try_send_ptr(channel_id: i64, value_ptr: *mut u8) -> i64 {
    channel::channel_try_send_ptr(channel_id, value_ptr)
}

/// Phase B pointer-payload receive; writes the dequeued pointer to `out_ptr` on `STATUS_OK`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_receive_ptr(channel_id: i64, out_ptr: *mut *mut u8) -> i64 {
    channel::channel_receive_ptr(channel_id, out_ptr)
}

/// Phase B pointer-payload try-receive; see [`channel::channel_try_receive_ptr`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn channel_try_receive_ptr(channel_id: i64, out_ptr: *mut *mut u8) -> i64 {
    channel::channel_try_receive_ptr(channel_id, out_ptr)
}
