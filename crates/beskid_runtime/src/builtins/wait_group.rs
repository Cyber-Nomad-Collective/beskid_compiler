use crate::wait_group;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn wait_group_create() -> i64 {
    wait_group::wait_group_create()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn wait_group_add(group_id: i64, delta: i64) {
    wait_group::wait_group_add(group_id, delta);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn wait_group_done(group_id: i64) {
    wait_group::wait_group_done(group_id);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn wait_group_wait(group_id: i64) -> i64 {
    wait_group::wait_group_wait(group_id)
}
