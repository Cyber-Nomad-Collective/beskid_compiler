use crate::mutex;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn mutex_create() -> i64 {
    mutex::mutex_create()
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn mutex_lock(mutex_id: i64) -> i64 {
    mutex::mutex_lock(mutex_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn mutex_try_lock(mutex_id: i64) -> i64 {
    mutex::mutex_try_lock(mutex_id)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn mutex_unlock(mutex_id: i64) {
    mutex::mutex_unlock(mutex_id);
}
