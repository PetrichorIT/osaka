use crate::{
    scope,
    task::JoinHandle,
    util::error::{CONTEXT_MISSING_ERROR, THREAD_LOCAL_DESTROYED_ERROR},
};

use super::{context, spawner::Spawner};
use std::{error, fmt, future::Future, marker::PhantomData};

#[derive(Debug, Clone)]
pub struct Handle {
    pub(super) spawner: Spawner,
}

impl Handle {
    pub fn enter(&self) -> EnterGuard<'_> {
        scope!("Handle::enter" =>
            EnterGuard {
                _guard: context::enter(self.clone()),
                _handle_lifetime: PhantomData,
            }
        )
    }

    #[track_caller]
    pub fn current() -> Self {
        context::current()
    }

    pub fn try_current() -> Result<Self, TryCurrentError> {
        context::try_current()
    }

    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.spawn_named(future, None)
    }

    #[track_caller]
    pub(crate) fn spawn_named<F>(&self, future: F, _name: Option<&str>) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let id = crate::runtime::task::Id::next();
        self.spawner.spawn(future, id)
    }
}

#[derive(Debug)]
pub struct HandleInner {}

pub struct EnterGuard<'a> {
    _guard: context::EnterGuard,
    _handle_lifetime: PhantomData<&'a Handle>,
}

/// Error returned by `try_current` when no Runtime has been started
#[derive(Debug)]
pub struct TryCurrentError {
    kind: TryCurrentErrorKind,
}

impl TryCurrentError {
    pub(crate) fn new_no_context() -> Self {
        Self {
            kind: TryCurrentErrorKind::NoContext,
        }
    }

    pub(crate) fn new_thread_local_destroyed() -> Self {
        Self {
            kind: TryCurrentErrorKind::ThreadLocalDestroyed,
        }
    }

    /// Returns true if the call failed because there is currently no runtime in
    /// the Osaka context.
    pub fn is_missing_context(&self) -> bool {
        matches!(self.kind, TryCurrentErrorKind::NoContext)
    }

    /// Returns true if the call failed because the Osaka context thread-local
    /// had been destroyed. This can usually only happen if in the destructor of
    /// other thread-locals.
    pub fn is_thread_local_destroyed(&self) -> bool {
        matches!(self.kind, TryCurrentErrorKind::ThreadLocalDestroyed)
    }
}

enum TryCurrentErrorKind {
    NoContext,
    ThreadLocalDestroyed,
}

impl fmt::Debug for TryCurrentErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TryCurrentErrorKind::*;
        match self {
            NoContext => f.write_str("NoContext"),
            ThreadLocalDestroyed => f.write_str("ThreadLocalDestroyed"),
        }
    }
}

impl fmt::Display for TryCurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TryCurrentErrorKind::*;
        match self.kind {
            NoContext => f.write_str(CONTEXT_MISSING_ERROR),
            ThreadLocalDestroyed => f.write_str(THREAD_LOCAL_DESTROYED_ERROR),
        }
    }
}

impl error::Error for TryCurrentError {}
