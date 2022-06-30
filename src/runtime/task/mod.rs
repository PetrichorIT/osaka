//! The task module.
//!
//! The task module contains the code that manages spawned tasks and provides a
//! safe API for the rest of the runtime to use. Each task in a runtime is
//! stored in an OwnedTasks or LocalOwnedTasks object.
//!
//! # Task reference types
//!
//! A task is usually referenced by multiple handles, and there are several
//! types of handles.
//!
//!  * OwnedTask - tasks stored in an OwnedTasks or LocalOwnedTasks are of this
//!    reference type.
//!
//!  * JoinHandle - each task has a JoinHandle that allows access to the output
//!    of the task.
//!
//!  * Waker - every waker for a task has this reference type. There can be any
//!    number of waker references.
//!
//!  * Notified - tracks whether the task is notified.
//!
//!  * Unowned - this task reference type is used for tasks not stored in any
//!    runtime. Mainly used for blocking tasks, but also in tests.
//!
//! The task uses a reference count to keep track of how many active references
//! exist. The Unowned reference type takes up two ref-counts. All other
//! reference types take up a single ref-count.
//!
//! Besides the waker type, each task has at most one of each reference type.
//!
//! # State
//!
//! The task stores its state in an atomic usize with various bitfields for the
//! necessary information. The state has the following bitfields:
//!
//!  * RUNNING - Tracks whether the task is currently being polled or cancelled.
//!    This bit functions as a lock around the task.
//!
//!  * COMPLETE - Is one once the future has fully completed and has been
//!    dropped. Never unset once set. Never set together with RUNNING.
//!
//!  * NOTIFIED - Tracks whether a Notified object currently exists.
//!
//!  * CANCELLED - Is set to one for tasks that should be cancelled as soon as
//!    possible. May take any value for completed tasks.
//!
//!  * JOIN_INTEREST - Is set to one if there exists a JoinHandle.
//!
//!  * JOIN_WAKER - Is set to one if the JoinHandle has set a waker.
//!
//! The rest of the bits are used for the ref-count.
//!
//! # Fields in the task
//!
//! The task has various fields. This section describes how and when it is safe
//! to access a field.
//!
//!  * The state field is accessed with atomic instructions.
//!
//!  * The OwnedTask reference has exclusive access to the `owned` field.
//!
//!  * The Notified reference has exclusive access to the `queue_next` field.
//!
//!  * The `owner_id` field can be set as part of construction of the task, but
//!    is otherwise immutable and anyone can access the field immutably without
//!    synchronization.
//!
//!  * If COMPLETE is one, then the JoinHandle has exclusive access to the
//!    stage field. If COMPLETE is zero, then the RUNNING bitfield functions as
//!    a lock for the stage field, and it can be accessed only by the thread
//!    that set RUNNING to one.
//!
//!  * If JOIN_WAKER is zero, then the JoinHandle has exclusive access to the
//!    join handle waker. If JOIN_WAKER and COMPLETE are both one, then the
//!    thread that set COMPLETE to one has exclusive access to the join handle
//!    waker.
//!
//! All other fields are immutable and can be accessed immutably without
//! synchronization by anyone.
//!
//! # Safety
//!
//! This section goes through various situations and explains why the API is
//! safe in that situation.
//!
//! ## Polling or dropping the future
//!
//! Any mutable access to the future happens after obtaining a lock by modifying
//! the RUNNING field, so exclusive access is ensured.
//!
//! When the task completes, exclusive access to the output is transferred to
//! the JoinHandle. If the JoinHandle is already dropped when the transition to
//! complete happens, the thread performing that transition retains exclusive
//! access to the output and should immediately drop it.
//!
//! ## Non-Send futures
//!
//! If a future is not Send, then it is bound to a LocalOwnedTasks.  The future
//! will only ever be polled or dropped given a LocalNotified or inside a call
//! to LocalOwnedTasks::shutdown_all. In either case, it is guaranteed that the
//! future is on the right thread.
//!
//! If the task is never removed from the LocalOwnedTasks, then it is leaked, so
//! there is no risk that the task is dropped on some other thread when the last
//! ref-count drops.
//!
//! ## Non-Send output
//!
//! When a task completes, the output is placed in the stage of the task. Then,
//! a transition that sets COMPLETE to true is performed, and the value of
//! JOIN_INTEREST when this transition happens is read.
//!
//! If JOIN_INTEREST is zero when the transition to COMPLETE happens, then the
//! output is immediately dropped.
//!
//! If JOIN_INTEREST is one when the transition to COMPLETE happens, then the
//! JoinHandle is responsible for cleaning up the output. If the output is not
//! Send, then this happens:
//!
//!  1. The output is created on the thread that the future was polled on. Since
//!     only non-Send futures can have non-Send output, the future was polled on
//!     the thread that the future was spawned from.
//!  2. Since JoinHandle<Output> is not Send if Output is not Send, the
//!     JoinHandle is also on the thread that the future was spawned from.
//!  3. Thus, the JoinHandle will not move the output across threads when it
//!     takes or drops the output.
//!
//! ## Recursive poll/shutdown
//!
//! Calling poll from inside a shutdown call or vice-versa is not prevented by
//! the API exposed by the task module, so this has to be safe. In either case,
//! the lock in the RUNNING bitfield makes the inner call return immediately. If
//! the inner call is a `shutdown` call, then the CANCELLED bit is set, and the
//! poll call will notice it when the poll finishes, and the task is cancelled
//! at that point.

// Some task infrastructure is here to support `JoinSet`, which is currently
// unstable. This should be removed once `JoinSet` is stabilized.

mod core;
use crate::util::Link;
use crate::util::Pointers;

use self::core::Cell;
use self::core::Header;

mod error;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::error::JoinError;

mod harness;
use self::harness::Harness;

mod join;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::join::JoinHandle;

mod list;
pub(crate) use self::list::{LocalOwnedTasks, OwnedTasks};

mod raw;
use self::raw::RawTask;

mod state;
use self::state::State;

mod waker;

use std::future::Future;

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::Arc;
use std::{fmt, mem};

use super::spawner::Shared;

/// An opaque ID that uniquely identifies a task relative to all other currently
/// running tasks.
///
/// # Notes
///
/// - Task IDs are unique relative to other *currently running* tasks. When a
///   task completes, the same ID may be used for another task.
/// - Task IDs are *not* sequential, and do not indicate the order in which
///   tasks are spawned, what runtime a task is spawned on, or any other data.
///
///
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct Id(u64);

/// An owned handle to the task, tracked by ref count.
#[repr(transparent)]
pub(crate) struct Task {
    raw: RawTask,
}

unsafe impl Send for Task {}
unsafe impl Sync for Task {}

/// A task was notified.
#[repr(transparent)]
pub(crate) struct Notified(Task);

// safety: This type cannot be used to touch the task without first verifying
// that the value is on a thread where it is safe to poll the task.
unsafe impl Send for Notified {}
unsafe impl Sync for Notified {}

/// A non-Send variant of Notified with the invariant that it is on a thread
/// where it is safe to poll it.
#[repr(transparent)]
pub(crate) struct LocalNotified {
    task: Task,
    _not_send: PhantomData<*const ()>,
}

/// A task that is not owned by any OwnedTasks. Used for blocking tasks.
/// This type holds two ref-counts.
pub(crate) struct UnownedTask {
    raw: RawTask,
}

// safety: This type can only be created given a Send task.
unsafe impl Send for UnownedTask {}
unsafe impl Sync for UnownedTask {}

/// Task result sent back.
pub(crate) type Result<T> = std::result::Result<T, JoinError>;

pub(crate) trait Schedule: Sync + Sized + 'static {
    /// The task has completed work and is ready to be released. The scheduler
    /// should release it immediately and return it. The task module will batch
    /// the ref-dec with setting other options.
    ///
    /// If the scheduler has already released the task, then None is returned.
    fn release(&self, task: &Task) -> Option<Task>;

    /// Schedule the task
    fn schedule(&self, task: Notified);

    /// Schedule the task to run in the near future, yielding the thread to
    /// other tasks.
    fn yield_now(&self, task: Notified) {
        self.schedule(task);
    }

    /// Polling the task resulted in a panic. Should the runtime shutdown?
    fn unhandled_panic(&self) {
        // By default, do nothing. This maintains the 1.0 behavior.
    }
}

/// This is the constructor for a new task. Three references to the task are
/// created. The first task reference is usually put into an OwnedTasks
/// immediately. The Notified is sent to the scheduler as an ordinary
/// notification.
fn new_task<T>(task: T, scheduler: Arc<Shared>, id: Id) -> (Task, Notified, JoinHandle<T::Output>)
where
    T: Future + 'static,
    T::Output: 'static,
{
    let raw = RawTask::new::<T>(task, scheduler, id.clone());
    let task = Task { raw };
    let notified = Notified(Task { raw });
    let join = JoinHandle::new(raw, id);

    (task, notified, join)
}

/// Creates a new task with an associated join handle. This method is used
/// only when the task is not going to be stored in an `OwnedTasks` list.
///
/// Currently only blocking tasks use this method.
#[allow(dead_code)]
pub(crate) fn unowned<T, S>(
    task: T,
    scheduler: Arc<Shared>,
    id: Id,
) -> (UnownedTask, JoinHandle<T::Output>)
where
    S: Schedule,
    T: Send + Future + 'static,
    T::Output: Send + 'static,
{
    let (task, notified, join) = new_task(task, scheduler, id);

    // This transfers the ref-count of task and notified into an UnownedTask.
    // This is valid because an UnownedTask holds two ref-counts.
    let unowned = UnownedTask { raw: task.raw };
    std::mem::forget(task);
    std::mem::forget(notified);

    (unowned, join)
}

impl Task {
    unsafe fn from_raw(ptr: NonNull<Header>) -> Task {
        Task {
            raw: RawTask::from_raw(ptr),
        }
    }

    fn header(&self) -> &Header {
        self.raw.header()
    }
}

impl Notified {
    fn header(&self) -> &Header {
        self.0.header()
    }
}

impl Task {
    /// Pre-emptively cancels the task as part of the shutdown process.
    pub(crate) fn shutdown(self) {
        let raw = self.raw;
        mem::forget(self);
        raw.shutdown();
    }
}

impl LocalNotified {
    /// Runs the task.
    pub(crate) fn run(self) {
        let raw = self.task.raw;
        mem::forget(self);
        raw.poll();
    }
}

impl UnownedTask {
    // Used in test of the inject queue.
    #[cfg(test)]
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    #[allow(dead_code)]
    pub(super) fn into_notified(self) -> Notified {
        Notified(self.into_task())
    }

    #[allow(dead_code)]
    fn into_task(self) -> Task {
        // Convert into a task.
        let task = Task { raw: self.raw };
        mem::forget(self);

        // Drop a ref-count since an UnownedTask holds two.
        task.header().state.ref_dec();

        task
    }

    #[allow(dead_code)]
    pub(crate) fn run(self) {
        let raw = self.raw;
        mem::forget(self);

        // Transfer one ref-count to a Task object.
        let task = Task { raw };

        // Use the other ref-count to poll the task.
        raw.poll();
        // Decrement our extra ref-count
        drop(task);
    }

    #[allow(dead_code)]
    pub(crate) fn shutdown(self) {
        self.into_task().shutdown()
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        // Decrement the ref count
        if self.header().state.ref_dec() {
            // Deallocate if this is the final ref count
            self.raw.dealloc();
        }
    }
}

impl Drop for UnownedTask {
    fn drop(&mut self) {
        // Decrement the ref count
        if self.raw.header().state.ref_dec_twice() {
            // Deallocate if this is the final ref count
            self.raw.dealloc();
        }
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Task({:p})", self.header())
    }
}

impl fmt::Debug for Notified {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "task::Notified({:p})", self.0.header())
    }
}

/// # Safety
///
/// Tasks are pinned.
unsafe impl Link for Task {
    type Handle = Task;
    type Target = Header;

    fn as_raw(handle: &Task) -> NonNull<Header> {
        handle.raw.header_ptr()
    }

    unsafe fn from_raw(ptr: NonNull<Header>) -> Task {
        Task::from_raw(ptr)
    }

    unsafe fn pointers(target: NonNull<Header>) -> NonNull<Pointers<Header>> {
        // Not super great as it avoids some of looms checking...
        NonNull::from(target.as_ref().owned.with_mut(|ptr| &mut *ptr))
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Id {
    // When 64-bit atomics are available, use a static `AtomicU64` counter to
    // generate task IDs.
    //
    // Note(eliza): we _could_ just use `crate::loom::AtomicU64`, which switches
    // between an atomic and mutex-based implementation here, rather than having
    // two separate functions for targets with and without 64-bit atomics.
    // However, because we can't use the mutex-based implementation in a static
    // initializer directly, the 32-bit impl also has to use a `OnceCell`, and I
    // thought it was nicer to avoid the `OnceCell` overhead on 64-bit
    // platforms...
    pub(crate) fn next() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Relaxed))
    }

    #[allow(dead_code)]
    pub(crate) fn as_u64(&self) -> u64 {
        self.0
    }
}
