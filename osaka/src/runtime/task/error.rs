use std::any::Any;
use std::fmt;
use std::io;

use super::Id;
use crate::util::SyncWrapper;
/// Task failed to execute to completion.
pub struct JoinError {
    repr: Repr,
    id: Id,
}

enum Repr {
    Cancelled,
    Panic(SyncWrapper<Box<dyn Any + Send + 'static>>),
}

impl JoinError {
    pub(crate) fn cancelled(id: Id) -> JoinError {
        JoinError {
            repr: Repr::Cancelled,
            id,
        }
    }

    pub(crate) fn panic(id: Id, err: Box<dyn Any + Send + 'static>) -> JoinError {
        JoinError {
            repr: Repr::Panic(SyncWrapper::new(err)),
            id,
        }
    }

    /// Returns true if the error was caused by the task being cancelled.
    pub fn is_cancelled(&self) -> bool {
        matches!(&self.repr, Repr::Cancelled)
    }

    /// Returns true if the error was caused by the task panicking.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::panic;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let err = osaka::spawn(async {
    ///         panic!("boom");
    ///     }).await.unwrap_err();
    ///
    ///     assert!(err.is_panic());
    /// }
    /// ```
    pub fn is_panic(&self) -> bool {
        matches!(&self.repr, Repr::Panic(_))
    }

    /// Consumes the join error, returning the object with which the task panicked.
    ///
    /// # Panics
    ///
    /// `into_panic()` panics if the `Error` does not represent the underlying
    /// task terminating with a panic. Use `is_panic` to check the error reason
    /// or `try_into_panic` for a variant that does not panic.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use std::panic;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let err = osaka::spawn(async {
    ///         panic!("boom");
    ///     }).await.unwrap_err();
    ///
    ///     if err.is_panic() {
    ///         // Resume the panic on the main task
    ///         panic::resume_unwind(err.into_panic());
    ///     }
    /// }
    /// ```
    #[track_caller]
    pub fn into_panic(self) -> Box<dyn Any + Send + 'static> {
        self.try_into_panic()
            .expect("`JoinError` reason is not a panic.")
    }

    /// Consumes the join error, returning the object with which the task
    /// panicked if the task terminated due to a panic. Otherwise, `self` is
    /// returned.
    ///
    /// # Examples
    ///
    /// ```should_panic
    /// use std::panic;
    ///
    /// #[osaka::main]
    /// async fn main() {
    ///     let err = osaka::spawn(async {
    ///         panic!("boom");
    ///     }).await.unwrap_err();
    ///
    ///     if let Ok(reason) = err.try_into_panic() {
    ///         // Resume the panic on the main task
    ///         panic::resume_unwind(reason);
    ///     }
    /// }
    /// ```
    pub fn try_into_panic(self) -> Result<Box<dyn Any + Send + 'static>, JoinError> {
        match self.repr {
            Repr::Panic(p) => Ok(p.into_inner()),
            _ => Err(self),
        }
    }
}

impl fmt::Display for JoinError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.repr {
            Repr::Cancelled => write!(fmt, "task {} was cancelled", self.id),
            Repr::Panic(_) => write!(fmt, "task {} panicked", self.id),
        }
    }
}

impl fmt::Debug for JoinError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.repr {
            Repr::Cancelled => write!(fmt, "JoinError::Cancelled({:?})", self.id),
            Repr::Panic(_) => write!(fmt, "JoinError::Panic({:?}, ...)", self.id),
        }
    }
}

impl std::error::Error for JoinError {}

impl From<JoinError> for io::Error {
    fn from(src: JoinError) -> io::Error {
        io::Error::new(
            io::ErrorKind::Other,
            match src.repr {
                Repr::Cancelled => "task was cancelled",
                Repr::Panic(_) => "task panicked",
            },
        )
    }
}
