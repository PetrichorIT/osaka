use crate::runtime::spawner::Shared;
use crate::runtime::task::{Cell, Harness, Header, Id, State};
use std::future::Future;

use std::ptr::NonNull;
use std::sync::Arc;
use std::task::{Poll, Waker};

/// Raw task handle
pub(super) struct RawTask {
    ptr: NonNull<Header>,
}

pub(super) struct Vtable {
    /// Polls the future.
    pub(super) poll: unsafe fn(NonNull<Header>),

    /// Deallocates the memory.
    pub(super) dealloc: unsafe fn(NonNull<Header>),

    /// Reads the task output, if complete.
    pub(super) try_read_output: unsafe fn(NonNull<Header>, *mut (), &Waker),

    /// Try to set the waker notified when the task is complete. Returns true if
    /// the task has already completed. If this call returns false, then the
    /// waker will not be notified.
    pub(super) try_set_join_waker: unsafe fn(NonNull<Header>, &Waker) -> bool,

    /// The join handle has been dropped.
    pub(super) drop_join_handle_slow: unsafe fn(NonNull<Header>),

    /// An abort handle has been dropped.
    #[allow(dead_code)]
    pub(super) drop_abort_handle: unsafe fn(NonNull<Header>),

    /// The task is remotely aborted.
    pub(super) remote_abort: unsafe fn(NonNull<Header>),

    /// Scheduler is being shutdown.
    pub(super) shutdown: unsafe fn(NonNull<Header>),
}

/// Get the vtable for the requested `T` and `S` generics.
pub(super) fn vtable<T: Future>() -> &'static Vtable {
    &Vtable {
        poll: poll::<T>,
        dealloc: dealloc::<T>,
        try_read_output: try_read_output::<T>,
        try_set_join_waker: try_set_join_waker::<T>,
        drop_join_handle_slow: drop_join_handle_slow::<T>,
        drop_abort_handle: drop_abort_handle::<T>,
        remote_abort: remote_abort::<T>,
        shutdown: shutdown::<T>,
    }
}

impl RawTask {
    pub(super) fn new<T>(task: T, scheduler: Arc<Shared>, id: Id) -> RawTask
    where
        T: Future,
    {
        let ptr = Box::into_raw(Cell::<_>::new(task, scheduler, State::new(), id));
        let ptr = unsafe { NonNull::new_unchecked(ptr as *mut Header) };

        RawTask { ptr }
    }

    pub(super) unsafe fn from_raw(ptr: NonNull<Header>) -> RawTask {
        RawTask { ptr }
    }

    pub(super) fn header_ptr(&self) -> NonNull<Header> {
        self.ptr
    }

    /// Returns a reference to the task's meta structure.
    ///
    /// Safe as `Header` is `Sync`.
    pub(super) fn header(&self) -> &Header {
        unsafe { self.ptr.as_ref() }
    }

    /// Safety: mutual exclusion is required to call this function.
    pub(super) fn poll(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.poll)(self.ptr) }
    }

    pub(super) fn dealloc(self) {
        let vtable = self.header().vtable;
        unsafe {
            (vtable.dealloc)(self.ptr);
        }
    }

    /// Safety: `dst` must be a `*mut Poll<super::Result<T::Output>>` where `T`
    /// is the future stored by the task.
    pub(super) unsafe fn try_read_output(self, dst: *mut (), waker: &Waker) {
        let vtable = self.header().vtable;
        (vtable.try_read_output)(self.ptr, dst, waker);
    }

    pub(super) fn try_set_join_waker(self, waker: &Waker) -> bool {
        let vtable = self.header().vtable;
        unsafe { (vtable.try_set_join_waker)(self.ptr, waker) }
    }

    pub(super) fn drop_join_handle_slow(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.drop_join_handle_slow)(self.ptr) }
    }

    #[allow(dead_code)]
    pub(super) fn drop_abort_handle(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.drop_abort_handle)(self.ptr) }
    }

    pub(super) fn shutdown(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.shutdown)(self.ptr) }
    }

    pub(super) fn remote_abort(self) {
        let vtable = self.header().vtable;
        unsafe { (vtable.remote_abort)(self.ptr) }
    }

    /// Increment the task's reference count.
    ///
    /// Currently, this is used only when creating an `AbortHandle`.
    #[allow(dead_code)]
    pub(super) fn ref_inc(self) {
        self.header().state.ref_inc();
    }
}

impl Clone for RawTask {
    fn clone(&self) -> Self {
        RawTask { ptr: self.ptr }
    }
}

impl Copy for RawTask {}

unsafe fn poll<T: Future>(ptr: NonNull<Header>) {
    let harness = Harness::<T>::from_raw(ptr);
    harness.poll();
}

unsafe fn dealloc<T: Future>(ptr: NonNull<Header>) {
    let harness = Harness::<T>::from_raw(ptr);
    harness.dealloc();
}

unsafe fn try_read_output<T: Future>(ptr: NonNull<Header>, dst: *mut (), waker: &Waker) {
    let out = &mut *(dst as *mut Poll<super::Result<T::Output>>);

    let harness = Harness::<T>::from_raw(ptr);
    harness.try_read_output(out, waker);
}

unsafe fn try_set_join_waker<T: Future>(ptr: NonNull<Header>, waker: &Waker) -> bool {
    let harness = Harness::<T>::from_raw(ptr);
    harness.try_set_join_waker(waker)
}

unsafe fn drop_join_handle_slow<T: Future>(ptr: NonNull<Header>) {
    let harness = Harness::<T>::from_raw(ptr);
    harness.drop_join_handle_slow()
}

unsafe fn drop_abort_handle<T: Future>(ptr: NonNull<Header>) {
    let harness = Harness::<T>::from_raw(ptr);
    harness.drop_reference();
}

unsafe fn remote_abort<T: Future>(ptr: NonNull<Header>) {
    let harness = Harness::<T>::from_raw(ptr);
    harness.remote_abort()
}

unsafe fn shutdown<T: Future>(ptr: NonNull<Header>) {
    let harness = Harness::<T>::from_raw(ptr);
    harness.shutdown()
}
