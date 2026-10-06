//! Private, opening-thread-bound ownership for source-issued failure admission.
//! Neither symbol strings nor managed pointers escape through a public packet API.
use super::{AotResult, invalid};
use beskid_artifacts::native_host::NativeHostLease;
use std::{
    ffi::{CStr, c_void},
    marker::PhantomData,
    rc::Rc,
    thread::ThreadId,
};

type Admit = unsafe extern "C" fn() -> usize;
type Resolve = unsafe extern "C" fn(usize) -> *mut c_void;
type Unroot = unsafe extern "C" fn(usize);
type Register = unsafe extern "C" fn(*mut *mut c_void) -> u8;
type Unregister = unsafe extern "C" fn(*mut *mut c_void);

/// Parent loader retains this before publishing its domain and destroys it before its host.
/// Constructible only inside the private Glue implementation from a producer-issued symbol.
pub(super) struct ReservedFailureDomain {
    handle: usize,
    resolve: Resolve,
    unroot: Unroot,
    register: Register,
    unregister: Unregister,
    thread: ThreadId,
    _bound: PhantomData<Rc<()>>,
}
impl ReservedFailureDomain {
    /// Caller must have verified the admission symbol against the current private source plan.
    /// Integrity and symbol existence alone do not issue that proof.
    pub(super) unsafe fn admit(lease: &NativeHostLease, image: usize, symbol: &CStr) -> AotResult<Self> {
        lease.images().verify_integrity().map_err(|error| invalid(error.to_string()))?;
        let admit = *unsafe { lease.images().image_symbol::<Admit>(image, symbol) }
            .map_err(|error| invalid(error.to_string()))?;
        let resolve = *unsafe { lease.images().provider_symbol::<Resolve>(c"gc_resolve_handle") }
            .map_err(|error| invalid(error.to_string()))?;
        let unroot = *unsafe { lease.images().provider_symbol::<Unroot>(c"gc_unroot_handle") }
            .map_err(|error| invalid(error.to_string()))?;
        let register = *unsafe { lease.images().provider_symbol::<Register>(c"beskid_rt_v5_gc_try_register_root") }
            .map_err(|error| invalid(error.to_string()))?;
        let unregister = *unsafe { lease.images().provider_symbol::<Unregister>(c"gc_unregister_root") }
            .map_err(|error| invalid(error.to_string()))?;
        let handle = unsafe { admit() };
        if handle == 0 {
            return Err(invalid("reserved failure admission exhausted before callable publication"));
        }
        if unsafe { resolve(handle) }.is_null() {
            unsafe { unroot(handle) };
            return Err(invalid("reserved failure admission returned an invalid canonical root"));
        }
        Ok(Self {
            handle,
            resolve,
            unroot,
            register,
            unregister,
            thread: std::thread::current().id(),
            _bound: PhantomData,
        })
    }
    /// This must run before evaluating invocation arguments. A failed registration publishes
    /// nothing and never begins checked work. Box fixes the address through moves of this lease.
    pub(super) fn destination(&self) -> AotResult<RootedFailureDestination<'_>> {
        if self.thread != std::thread::current().id() || self.handle == 0 {
            return Err(invalid("reserved failure domain is outside its opening lifetime/thread"));
        }
        let mut slot = Box::new(std::ptr::null_mut::<c_void>());
        let address = std::ptr::from_mut(slot.as_mut());
        if unsafe { (self.register)(address) } == 0 {
            return Err(invalid("caller result root admission exhausted before arguments"));
        }
        Ok(RootedFailureDestination { owner: self, slot })
    }
    pub(super) fn close(&mut self) -> AotResult<()> {
        if self.thread != std::thread::current().id() {
            return Err(invalid("reserved domain changed threads"));
        }
        if self.handle != 0 {
            unsafe { (self.unroot)(self.handle) };
            self.handle = 0;
        }
        Ok(())
    }
}
impl Drop for ReservedFailureDomain {
    fn drop(&mut self) {
        // Rc marker prevents a safe cross-thread move. Parent retains the live host here.
        if self.thread == std::thread::current().id() && self.handle != 0 {
            unsafe { (self.unroot)(self.handle) };
            self.handle = 0;
        }
    }
}
pub(super) struct RootedFailureDestination<'a> {
    owner: &'a ReservedFailureDomain,
    slot: Box<*mut c_void>,
}
impl RootedFailureDestination<'_> {
    pub(super) fn failure_handle(&self) -> usize {
        self.owner.handle
    }
    pub(super) fn address(&mut self) -> *mut *mut c_void {
        std::ptr::from_mut(self.slot.as_mut())
    }
    /// Caller consumes/marshals this while the lease still roots it; no unrooted pointer API.
    pub(super) fn with_published<T>(&self, consume: impl FnOnce(*mut c_void) -> AotResult<T>) -> AotResult<T> {
        if self.owner.thread != std::thread::current().id() || self.slot.is_null() {
            return Err(invalid("checked invocation did not publish a rooted result"));
        }
        // A live failure handle is also checked at the private wrapper before work starts.
        if unsafe { (self.owner.resolve)(self.owner.handle) }.is_null() {
            return Err(invalid("reserved failure root expired"));
        }
        consume(*self.slot)
    }
}
impl Drop for RootedFailureDestination<'_> {
    fn drop(&mut self) {
        unsafe { (self.owner.unregister)(std::ptr::from_mut(self.slot.as_mut())) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    thread_local! {
        static ROOT: Cell<usize> = const { Cell::new(0) };
        static REJECT: Cell<bool> = const { Cell::new(false) };
        static RELEASES: Cell<usize> = const { Cell::new(0) };
    }
    unsafe extern "C" fn register(slot: *mut *mut c_void) -> u8 {
        if REJECT.get() {
            return 0;
        }
        ROOT.set(slot as usize);
        1
    }
    unsafe extern "C" fn unregister(slot: *mut *mut c_void) {
        assert_eq!(ROOT.get(), slot as usize);
        ROOT.set(0);
    }
    unsafe extern "C" fn resolve(handle: usize) -> *mut c_void {
        if handle == 7 { std::ptr::dangling_mut::<u8>().cast() } else { std::ptr::null_mut() }
    }
    unsafe extern "C" fn unroot(handle: usize) {
        assert_eq!(handle, 7);
        RELEASES.set(RELEASES.get() + 1);
    }
    fn domain() -> ReservedFailureDomain {
        ROOT.set(0);
        REJECT.set(false);
        RELEASES.set(0);
        ReservedFailureDomain {
            handle: 7,
            resolve,
            unroot,
            register,
            unregister,
            thread: std::thread::current().id(),
            _bound: PhantomData,
        }
    }
    #[test]
    fn destination_address_survives_move_and_remains_rooted_through_consumption() {
        let mut owner = domain();
        let mut slot = owner.destination().unwrap();
        let address = slot.address();
        assert_eq!(ROOT.get(), address as usize);
        let mut moved = slot;
        assert_eq!(moved.address(), address);
        assert_eq!(moved.failure_handle(), 7);
        assert!(moved.with_published(|_| Ok(())).is_err());
        unsafe {
            *address = std::ptr::dangling_mut::<u8>().cast();
        }
        moved
            .with_published(|value| {
                assert!(!value.is_null());
                assert_eq!(ROOT.get(), address as usize);
                Ok(())
            })
            .unwrap();
        drop(moved);
        assert_eq!(ROOT.get(), 0);
        owner.close().unwrap();
        owner.close().unwrap();
        assert_eq!(RELEASES.get(), 1);
    }
    #[test]
    fn root_admission_failure_leaves_no_slot_and_retains_reserved_failure() {
        let owner = domain();
        REJECT.set(true);
        assert!(owner.destination().is_err());
        assert_eq!(ROOT.get(), 0);
        assert_eq!(RELEASES.get(), 0);
        drop(owner);
        assert_eq!(RELEASES.get(), 1);
    }
}
