use std::{
    cell::{RefCell, RefMut},
    future::Future,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::AtomicUsize,
    sync::atomic::Ordering::*,
    task::{Context, Poll, Poll::*, Waker},
};

use crate::{
    adapter::UnsafeCell,
    util::{Link, LinkedList, Pointers},
};

#[derive(Debug)]
pub struct Semaphore {
    waiters: RefCell<Waitlist>,
    state: AtomicUsize,
}

impl Semaphore {
    pub(crate) const MAX_PERMIT: usize = std::usize::MAX >> 3;
    pub(crate) const CLOSED: usize = 1;
    pub(crate) const PERMIT_SHIFT: usize = 1;

    pub fn new(permits: usize) -> Self {
        assert!(permits < Semaphore::MAX_PERMIT);

        let state = AtomicUsize::new(permits << Semaphore::PERMIT_SHIFT);

        Self {
            state,
            waiters: RefCell::new(Waitlist {
                list: LinkedList::new(),
                closed: false,
            }),
        }
    }

    pub(crate) const fn const_new(mut permits: usize) -> Semaphore {
        // NOTE: assertions and by extension panics are still being worked on: https://github.com/rust-lang/rust/issues/74925
        // currently we just clamp the permit count when it exceeds the max
        permits &= Semaphore::MAX_PERMIT;

        Semaphore {
            state: AtomicUsize::new(permits << Self::PERMIT_SHIFT),
            waiters: RefCell::new(Waitlist {
                list: LinkedList::new(),
                closed: false,
            }),
        }
    }

    pub(crate) fn available_permits(&self) -> usize {
        self.state.load(Acquire) >> Semaphore::PERMIT_SHIFT
    }

    pub(crate) fn release(&self, added: usize) {
        if added == 0 {
            return;
        }

        self.add_permits_locked(added, self.waiters.borrow_mut())
    }

    pub fn close(&self) {
        let mut waiters = self.waiters.borrow_mut();
        self.state.fetch_or(Self::CLOSED, Release);
        waiters.closed = true;

        while let Some(mut waiter) = waiters.list.pop_back() {
            let waker = unsafe { waiter.as_mut().waker.with_mut(|w| (*w).take()) };
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.state.load(Acquire) & Self::CLOSED == Self::CLOSED
    }

    pub(crate) fn try_acquire(&self, num_permits: usize) -> Result<(), TryAcquireError> {
        assert!(num_permits < Semaphore::MAX_PERMIT);

        let num_permits = (num_permits as usize) << Self::PERMIT_SHIFT;
        let mut curr = self.state.load(Acquire);
        loop {
            if curr & Self::CLOSED == Self::CLOSED {
                return Err(TryAcquireError::Closed);
            }

            if curr < num_permits {
                return Err(TryAcquireError::NoPermits);
            }

            let next = curr - num_permits;
            match self.state.compare_exchange(curr, next, AcqRel, Acquire) {
                Ok(_) => {
                    return Ok(());
                }
                Err(actual) => curr = actual,
            }
        }
    }

    pub(crate) fn acquire(&self, num_permits: usize) -> Acquire<'_> {
        Acquire::new(self, num_permits)
    }

    pub(crate) fn add_permits_locked(&self, mut rem: usize, waiters: RefMut<'_, Waitlist>) {
        let mut wakers = Vec::new();
        let mut lock = Some(waiters);
        let mut is_empty = false;

        while rem > 0 {
            let mut waiters = lock.take().unwrap_or_else(|| self.waiters.borrow_mut());
            'inner: loop {
                match waiters.list.last() {
                    Some(waiter) => {
                        if !waiter.assign_permits(&mut rem) {
                            break 'inner;
                        }
                    }
                    None => {
                        is_empty = true;
                        break 'inner;
                    }
                }

                let mut waiter = waiters.list.pop_back().unwrap();
                if let Some(waker) = unsafe { waiter.as_mut().waker.with_mut(|w| (*w).take()) } {
                    wakers.push(waker)
                }
            }

            if rem > 0 && is_empty {
                let permits = rem;
                assert!(permits < Semaphore::MAX_PERMIT);

                let prev = self
                    .state
                    .fetch_add(rem << Semaphore::PERMIT_SHIFT, Release);
                let _prev = prev >> Semaphore::PERMIT_SHIFT;

                rem = 0;
            }

            drop(waiters);
            wakers.drain(..).for_each(|w| w.wake())
        }
    }

    fn poll_acquire(
        &self,
        cx: &mut Context<'_>,
        num_permits: usize,
        node: Pin<&mut Waiter>,
        queued: bool,
    ) -> Poll<Result<(), AcquireError>> {
        let mut acquired = 0;

        let needed = if queued {
            node.state.load(Acquire) << Self::PERMIT_SHIFT
        } else {
            (num_permits as usize) << Self::PERMIT_SHIFT
        };

        let mut lock = None;
        // First, try to take the requested number of permits from the
        // semaphore.
        let mut curr = self.state.load(Acquire);
        let mut waiters = loop {
            // Has the semaphore closed?
            if curr & Self::CLOSED > 0 {
                return Ready(Err(AcquireError::closed()));
            }

            let mut remaining = 0;
            let total = curr
                .checked_add(acquired)
                .expect("number of permits must not overflow");

            let (next, acq) = if total >= needed {
                // It the currently available and the allready aquired
                // are sufficient
                // Define the CAS swap value as the defined
                // and the acq is as needed.
                let next = curr - (needed - acquired);
                (next, needed >> Self::PERMIT_SHIFT)
            } else {
                // If not
                // Set remaining to be the onees missing from
                // the current context
                remaining = (needed - acquired) - curr;
                (0, curr >> Self::PERMIT_SHIFT)
            };

            if remaining > 0 && lock.is_none() {
                lock = Some(self.waiters.borrow_mut());
            }

            match self.state.compare_exchange(curr, next, AcqRel, Acquire) {
                Ok(_) => {
                    acquired += acq;
                    if remaining == 0 {
                        if !queued {
                            return Ready(Ok(()));
                        } else if lock.is_none() {
                            break self.waiters.borrow_mut();
                        }
                    }
                    break lock.expect("lock must be acquired before waiting");
                }
                Err(actual) => curr = actual,
            }
        };

        if waiters.closed {
            return Ready(Err(AcquireError::closed()));
        }

        if node.assign_permits(&mut acquired) {
            self.add_permits_locked(acquired, waiters);
            return Ready(Ok(()));
        }

        assert_eq!(acquired, 0);

        // Otherwise, register the waker & enqueue the node.
        node.waker.with_mut(|waker| {
            // Safety: the wait list is locked, so we may modify the waker.
            let waker = unsafe { &mut *waker };
            // Do we need to register the new waker?
            if waker
                .as_ref()
                .map(|waker| !waker.will_wake(cx.waker()))
                .unwrap_or(true)
            {
                *waker = Some(cx.waker().clone());
            }
        });

        // If the waiter is not already in the wait queue, enqueue it.
        if !queued {
            let node = unsafe {
                let node = Pin::into_inner_unchecked(node) as *mut _;
                NonNull::new_unchecked(node)
            };

            waiters.list.push_front(node);
        }
        Pending
    }
}

unsafe impl Sync for Semaphore {}

#[derive(Debug)]
pub(crate) struct Waitlist {
    list: LinkedList<Waiter, <Waiter as Link>::Target>,
    closed: bool,
}

#[derive(Debug, PartialEq)]
pub enum TryAcquireError {
    Closed,
    NoPermits,
}

impl TryAcquireError {
    /// Returns `true` if the error was caused by a closed semaphore.
    #[allow(dead_code)] // may be used later!
    pub(crate) fn is_closed(&self) -> bool {
        matches!(self, TryAcquireError::Closed)
    }

    /// Returns `true` if the error was caused by calling `try_acquire` on a
    /// semaphore with no available permits.
    #[allow(dead_code)] // may be used later!
    pub(crate) fn is_no_permits(&self) -> bool {
        matches!(self, TryAcquireError::NoPermits)
    }
}

impl std::fmt::Display for TryAcquireError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TryAcquireError::Closed => write!(fmt, "semaphore closed"),
            TryAcquireError::NoPermits => write!(fmt, "no permits available"),
        }
    }
}

impl std::error::Error for TryAcquireError {}

#[derive(Debug)]
pub struct AcquireError(());

impl AcquireError {
    fn closed() -> AcquireError {
        AcquireError(())
    }
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "semaphore closed")
    }
}

impl std::error::Error for AcquireError {}

pub(crate) struct Acquire<'a> {
    node: Waiter,
    semaphore: &'a Semaphore,
    num_permits: usize,
    queued: bool,
}

impl<'a> Acquire<'a> {
    pub fn new(semaphore: &'a Semaphore, num_permits: usize) -> Self {
        return Self {
            node: Waiter::new(num_permits),
            semaphore,
            num_permits,
            queued: false,
        };
    }

    fn project(self: Pin<&mut Self>) -> (Pin<&mut Waiter>, &Semaphore, usize, &mut bool) {
        fn is_unpin<T: Unpin>() {}
        unsafe {
            // Safety: all fields other than `node` are `Unpin`

            is_unpin::<&Semaphore>();
            is_unpin::<&mut bool>();
            is_unpin::<u32>();

            let this = self.get_unchecked_mut();
            (
                Pin::new_unchecked(&mut this.node),
                this.semaphore,
                this.num_permits,
                &mut this.queued,
            )
        }
    }
}

impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, semaphore, needed, queued) = self.project();

        let result = match semaphore.poll_acquire(cx, needed, node, *queued) {
            Pending => {
                *queued = true;
                Pending
            }
            Ready(r) => {
                // coop.made_progress();
                r?;
                *queued = false;
                Ready(Ok(()))
            }
        };

        return result;
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        if !self.queued {
            return;
        }

        let mut waiters = self.semaphore.waiters.borrow_mut();

        // remove the entry from the list
        let node = NonNull::from(&mut self.node);
        // Safety: we have locked the wait list.
        unsafe { waiters.list.remove(node) };

        let acquired_permits = self.num_permits as usize - self.node.state.load(Acquire);
        if acquired_permits > 0 {
            self.semaphore.add_permits_locked(acquired_permits, waiters);
        }
    }
}

unsafe impl Sync for Acquire<'_> {}

pub struct Waiter {
    state: AtomicUsize,
    waker: UnsafeCell<Option<Waker>>,
    pointers: Pointers<Waiter>,
}

impl Waiter {
    pub fn new(num_permits: usize) -> Self {
        Waiter {
            state: AtomicUsize::new(num_permits as usize),
            waker: UnsafeCell::new(None),
            pointers: Pointers::new(),
        }
    }

    pub fn assign_permits(&self, n: &mut usize) -> bool {
        let mut curr = self.state.load(Acquire);
        loop {
            let assign = std::cmp::min(curr, *n);
            let next = curr - assign;
            match self.state.compare_exchange(curr, next, AcqRel, Acquire) {
                Ok(_) => {
                    *n -= assign;
                    return next == 0;
                }
                Err(actual) => curr = actual,
            }
        }
    }
}

unsafe impl Link for Waiter {
    type Handle = NonNull<Waiter>;
    type Target = Waiter;

    fn as_raw(handle: &Self::Handle) -> NonNull<Self::Target> {
        *handle
    }

    unsafe fn from_raw(ptr: NonNull<Self::Target>) -> Self::Handle {
        ptr
    }

    unsafe fn pointers(
        mut target: NonNull<Self::Target>,
    ) -> NonNull<crate::util::Pointers<Self::Target>> {
        NonNull::from(&mut target.as_mut().pointers)
    }
}
