use crate::hub;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_create() -> i64 {
    hub::hub_create()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_register(hub_id: i64, index: i64, channel_id: i64) -> i64 {
    hub::hub_register(hub_id, index, channel_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_unregister(hub_id: i64, index: i64) -> i64 {
    hub::hub_unregister(hub_id, index)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_wait_receive(
    hub_id: i64,
    out_index: *mut i64,
    out_value: *mut i64,
) -> i64 {
    hub::hub_wait_receive(hub_id, out_index, out_value)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_wait_receive_status(hub_id: i64) -> i64 {
    hub::hub_wait_receive_status(hub_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_wait_receive_index(hub_id: i64) -> i64 {
    hub::hub_wait_receive_index(hub_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn hub_wait_receive_value(hub_id: i64) -> i64 {
    hub::hub_wait_receive_value(hub_id)
}
