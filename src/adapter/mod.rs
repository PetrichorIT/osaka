use std::sync::{MutexGuard, TryLockError};

#[derive(Debug)]
pub(crate) struct UnsafeCell<T>(std::cell::UnsafeCell<T>);

impl<T> UnsafeCell<T> {
    pub(crate) const fn new(data: T) -> UnsafeCell<T> {
        UnsafeCell(std::cell::UnsafeCell::new(data))
    }

    pub(crate) fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
        f(self.0.get())
    }

    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}

#[derive(Debug)]
pub(crate) struct Mutex<T: ?Sized>(std::sync::Mutex<T>);

#[allow(dead_code)]
impl<T> Mutex<T> {
    #[inline]
    pub(crate) fn new(t: T) -> Mutex<T> {
        Mutex(std::sync::Mutex::new(t))
    }

    #[inline]
    pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
        match self.0.lock() {
            Ok(guard) => guard,
            Err(p_err) => p_err.into_inner(),
        }
    }

    #[inline]
    pub(crate) fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        match self.0.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(p_err)) => Some(p_err.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }
}
