use super::batch_semaphore as ll; // low level implementation
use super::{AcquireError, TryAcquireError};
use std::sync::Arc;

/// Counting semaphore performing asynchronous permit acquisition.
///
/// A semaphore maintains a set of permits. Permits are used to synchronize
/// access to a shared resource. A semaphore differs from a mutex in that it
/// can allow more than one concurrent caller to access the shared resource at a
/// time.
///
/// When `acquire` is called and the semaphore has remaining permits, the
/// function immediately returns a permit. However, if no remaining permits are
/// available, `acquire` (asynchronously) waits until an outstanding permit is
/// dropped. At this point, the freed permit is assigned to the caller.
///
/// This `Semaphore` is fair, which means that permits are given out in the order
/// they were requested. This fairness is also applied when `acquire_many` gets
/// involved, so if a call to `acquire_many` at the front of the queue requests
/// more permits than currently available, this can prevent a call to `acquire`
/// from completing, even if the semaphore has enough permits complete the call
/// to `acquire`.
///
/// To use the `Semaphore` in a poll function, you can use the [`PollSemaphore`]
/// utility.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use osaka::sync::{Semaphore, TryAcquireError};
///
/// #[osaka::main]
/// async fn main() {
///     let semaphore = Semaphore::new(3);
///
///     let a_permit = semaphore.acquire().await.unwrap();
///     let two_permits = semaphore.acquire_many(2).await.unwrap();
///
///     assert_eq!(semaphore.available_permits(), 0);
///
///     let permit_attempt = semaphore.try_acquire();
///     assert_eq!(permit_attempt.err(), Some(TryAcquireError::NoPermits));
/// }
/// ```
///
/// Use [`Semaphore::acquire_owned`] to move permits across tasks:
///
/// ```
/// use std::sync::Arc;
/// use osaka::sync::Semaphore;
///
/// #[osaka::main]
/// async fn main() {
///     let semaphore = Arc::new(Semaphore::new(3));
///     let mut join_handles = Vec::new();
///
///     for _ in 0..5 {
///         let permit = semaphore.clone().acquire_owned().await.unwrap();
///         join_handles.push(osaka::spawn(async move {
///             // perform task...
///             // explicitly own `permit` in the task
///             drop(permit);
///         }));
///     }
///
///     for handle in join_handles {
///         handle.await.unwrap();
///     }
/// }
/// ```
///
/// [`PollSemaphore`]: https://docs.rs/tokio-util/0.6/tokio_util/sync/struct.PollSemaphore.html
/// [`Semaphore::acquire_owned`]: crate::sync::Semaphore::acquire_owned
#[derive(Debug)]
pub struct Semaphore {
    /// The low level semaphore
    ll_sem: ll::Semaphore,
}

impl Semaphore {
    /// Creates a new semaphore with the initial number of permits.
    #[track_caller]
    pub fn new(permits: usize) -> Self {
        let ll_sem = ll::Semaphore::new(permits);
        Self { ll_sem }
    }

    pub const fn const_new(permits: usize) -> Self {
        return Self {
            ll_sem: ll::Semaphore::const_new(permits),
        };
    }

    /// Returns the current number of available permits.
    pub fn available_permits(&self) -> usize {
        self.ll_sem.available_permits()
    }

    /// Adds `n` new permits to the semaphore.
    ///
    /// The maximum number of permits is `usize::MAX >> 3`, and this function will panic if the limit is exceeded.
    pub fn add_permits(&self, n: usize) {
        self.ll_sem.release(n);
    }

    /// Acquires a permit from the semaphore.
    ///
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`SemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute permits in the order they
    /// were requested. Cancelling a call to `acquire` makes you lose your place
    /// in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use osaka::sync::Semaphore;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let semaphore = Semaphore::new(2);
    ///
    ///     let permit_1 = semaphore.acquire().await.unwrap();
    ///     assert_eq!(semaphore.available_permits(), 1);
    ///
    ///     let permit_2 = semaphore.acquire().await.unwrap();
    ///     assert_eq!(semaphore.available_permits(), 0);
    ///
    ///     drop(permit_1);
    ///     assert_eq!(semaphore.available_permits(), 1);
    /// }
    /// ```
    ///
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
        let inner = self.ll_sem.acquire(1);

        inner.await?;
        Ok(SemaphorePermit {
            sem: self,
            permits: 1,
        })
    }

    /// Acquires `n` permits from the semaphore.
    ///
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`SemaphorePermit`] representing the
    /// acquired permits.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute permits in the order they
    /// were requested. Cancelling a call to `acquire_many` makes you lose your
    /// place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use osaka::sync::Semaphore;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let semaphore = Semaphore::new(5);
    ///
    ///     let permit = semaphore.acquire_many(3).await.unwrap();
    ///     assert_eq!(semaphore.available_permits(), 2);
    /// }
    /// ```
    ///
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub async fn acquire_many(&self, n: usize) -> Result<SemaphorePermit<'_>, AcquireError> {
        self.ll_sem.acquire(n).await?;

        Ok(SemaphorePermit {
            sem: self,
            permits: n,
        })
    }

    /// Tries to acquire a permit from the semaphore.
    ///
    /// If the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left. Otherwise,
    /// this returns a [`SemaphorePermit`] representing the acquired permits.
    ///
    /// # Examples
    ///
    /// ```
    /// use osaka::sync::{Semaphore, TryAcquireError};
    ///
    /// # fn main() {
    /// let semaphore = Semaphore::new(2);
    ///
    /// let permit_1 = semaphore.try_acquire().unwrap();
    /// assert_eq!(semaphore.available_permits(), 1);
    ///
    /// let permit_2 = semaphore.try_acquire().unwrap();
    /// assert_eq!(semaphore.available_permits(), 0);
    ///
    /// let permit_3 = semaphore.try_acquire();
    /// assert_eq!(permit_3.err(), Some(TryAcquireError::NoPermits));
    /// # }
    /// ```
    ///
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
                permits: 1,
            }),
            Err(e) => Err(e),
        }
    }

    /// Tries to acquire `n` permits from the semaphore.
    ///
    /// If the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are not enough permits left.
    /// Otherwise, this returns a [`SemaphorePermit`] representing the acquired permits.
    ///
    /// # Examples
    ///
    /// ```
    /// use osaka::sync::{Semaphore, TryAcquireError};
    ///
    /// # fn main() {
    /// let semaphore = Semaphore::new(4);
    ///
    /// let permit_1 = semaphore.try_acquire_many(3).unwrap();
    /// assert_eq!(semaphore.available_permits(), 1);
    ///
    /// let permit_2 = semaphore.try_acquire_many(2);
    /// assert_eq!(permit_2.err(), Some(TryAcquireError::NoPermits));
    /// # }
    /// ```
    ///
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`SemaphorePermit`]: crate::sync::SemaphorePermit
    pub fn try_acquire_many(&self, n: usize) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        match self.ll_sem.try_acquire(n) {
            Ok(_) => Ok(SemaphorePermit {
                sem: self,
                permits: n,
            }),
            Err(e) => Err(e),
        }
    }

    /// Acquires a permit from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method.
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute permits in the order they
    /// were requested. Cancelling a call to `acquire_owned` makes you lose your
    /// place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use osaka::sync::Semaphore;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(3));
    ///     let mut join_handles = Vec::new();
    ///
    ///     for _ in 0..5 {
    ///         let permit = semaphore.clone().acquire_owned().await.unwrap();
    ///         join_handles.push(osaka::spawn(async move {
    ///             // perform task...
    ///             // explicitly own `permit` in the task
    ///             drop(permit);
    ///         }));
    ///     }
    ///
    ///     for handle in join_handles {
    ///         handle.await.unwrap();
    ///     }
    /// }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub async fn acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, AcquireError> {
        let inner = self.ll_sem.acquire(1);

        inner.await?;
        Ok(OwnedSemaphorePermit {
            sem: self,
            permits: 1,
        })
    }

    /// Acquires `n` permits from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method.
    /// If the semaphore has been closed, this returns an [`AcquireError`].
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute permits in the order they
    /// were requested. Cancelling a call to `acquire_many_owned` makes you lose
    /// your place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use osaka::sync::Semaphore;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(10));
    ///     let mut join_handles = Vec::new();
    ///
    ///     for _ in 0..5 {
    ///         let permit = semaphore.clone().acquire_many_owned(2).await.unwrap();
    ///         join_handles.push(osaka::spawn(async move {
    ///             // perform task...
    ///             // explicitly own `permit` in the task
    ///             drop(permit);
    ///         }));
    ///     }
    ///
    ///     for handle in join_handles {
    ///         handle.await.unwrap();
    ///     }
    /// }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`AcquireError`]: crate::sync::AcquireError
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub async fn acquire_many_owned(
        self: Arc<Self>,
        n: usize,
    ) -> Result<OwnedSemaphorePermit, AcquireError> {
        let inner = self.ll_sem.acquire(n);

        inner.await?;
        Ok(OwnedSemaphorePermit {
            sem: self,
            permits: n,
        })
    }

    /// Tries to acquire a permit from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method. If
    /// the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left.
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use osaka::sync::{Semaphore, TryAcquireError};
    ///
    /// # fn main() {
    /// let semaphore = Arc::new(Semaphore::new(2));
    ///
    /// let permit_1 = Arc::clone(&semaphore).try_acquire_owned().unwrap();
    /// assert_eq!(semaphore.available_permits(), 1);
    ///
    /// let permit_2 = Arc::clone(&semaphore).try_acquire_owned().unwrap();
    /// assert_eq!(semaphore.available_permits(), 0);
    ///
    /// let permit_3 = semaphore.try_acquire_owned();
    /// assert_eq!(permit_3.err(), Some(TryAcquireError::NoPermits));
    /// # }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        match self.ll_sem.try_acquire(1) {
            Ok(_) => Ok(OwnedSemaphorePermit {
                sem: self,
                permits: 1,
            }),
            Err(e) => Err(e),
        }
    }

    /// Tries to acquire `n` permits from the semaphore.
    ///
    /// The semaphore must be wrapped in an [`Arc`] to call this method. If
    /// the semaphore has been closed, this returns a [`TryAcquireError::Closed`]
    /// and a [`TryAcquireError::NoPermits`] if there are no permits left.
    /// Otherwise, this returns a [`OwnedSemaphorePermit`] representing the
    /// acquired permit.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use osaka::sync::{Semaphore, TryAcquireError};
    ///
    /// # fn main() {
    /// let semaphore = Arc::new(Semaphore::new(4));
    ///
    /// let permit_1 = Arc::clone(&semaphore).try_acquire_many_owned(3).unwrap();
    /// assert_eq!(semaphore.available_permits(), 1);
    ///
    /// let permit_2 = semaphore.try_acquire_many_owned(2);
    /// assert_eq!(permit_2.err(), Some(TryAcquireError::NoPermits));
    /// # }
    /// ```
    ///
    /// [`Arc`]: std::sync::Arc
    /// [`TryAcquireError::Closed`]: crate::sync::TryAcquireError::Closed
    /// [`TryAcquireError::NoPermits`]: crate::sync::TryAcquireError::NoPermits
    /// [`OwnedSemaphorePermit`]: crate::sync::OwnedSemaphorePermit
    pub fn try_acquire_many_owned(
        self: Arc<Self>,
        n: usize,
    ) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        match self.ll_sem.try_acquire(n) {
            Ok(_) => Ok(OwnedSemaphorePermit {
                sem: self,
                permits: n,
            }),
            Err(e) => Err(e),
        }
    }

    /// Closes the semaphore.
    ///
    /// This prevents the semaphore from issuing new permits and notifies all pending waiters.
    ///
    /// # Examples
    ///
    /// ```
    /// use osaka::sync::Semaphore;
    /// use std::sync::Arc;
    /// use osaka::sync::TryAcquireError;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let semaphore = Arc::new(Semaphore::new(1));
    ///     let semaphore2 = semaphore.clone();
    ///
    ///     osaka::spawn(async move {
    ///         let permit = semaphore.acquire_many(2).await;
    ///         assert!(permit.is_err());
    ///         println!("waiter received error");
    ///     });
    ///
    ///     println!("closing semaphore");
    ///     semaphore2.close();
    ///
    ///     // Cannot obtain more permits
    ///     assert_eq!(semaphore2.try_acquire().err(), Some(TryAcquireError::Closed))
    /// }
    /// ```
    pub fn close(&self) {
        self.ll_sem.close();
    }

    /// Returns true if the semaphore is closed
    pub fn is_closed(&self) -> bool {
        self.ll_sem.is_closed()
    }
}

/// A permit from the semaphore.
///
/// This type is created by the [`acquire`] method.
///
/// [`acquire`]: crate::sync::Semaphore::acquire()
#[must_use]
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    sem: &'a Semaphore,
    permits: usize,
}

impl<'a> SemaphorePermit<'a> {
    /// Forgets the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    pub fn forget(mut self) {
        self.permits = 0;
    }
}

impl<'a> Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.sem.add_permits(self.permits as usize);
    }
}

/// An owned permit from the semaphore.
///
/// This type is created by the [`acquire_owned`] method.
///
/// [`acquire_owned`]: crate::sync::Semaphore::acquire_owned()
#[must_use]
#[derive(Debug)]
pub struct OwnedSemaphorePermit {
    sem: Arc<Semaphore>,
    permits: usize,
}

impl OwnedSemaphorePermit {
    /// Forgets the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    pub fn forget(mut self) {
        self.permits = 0;
    }
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        self.sem.add_permits(self.permits as usize);
    }
}
