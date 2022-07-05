use crate::scope;

use super::{
    handle::{Handle, TryCurrentError},
    spawner::Spawner,
};
use std::cell::RefCell;

thread_local! {
    pub(crate) static CONTEXT: RefCell<Option<Handle>> = RefCell::new(None);
}

pub(crate) fn try_current() -> Result<Handle, TryCurrentError> {
    match CONTEXT.try_with(|ctx| ctx.borrow().clone()) {
        Ok(Some(handle)) => Ok(handle),
        Ok(None) => Err(TryCurrentError::new_no_context()),
        Err(_access_error) => Err(TryCurrentError::new_thread_local_destroyed()),
    }
}

#[track_caller]
pub(crate) fn current() -> Handle {
    match try_current() {
        Ok(handle) => handle,
        Err(e) => panic!("{}", e),
    }
}

pub(crate) fn spawn_handle() -> Option<Spawner> {
    match CONTEXT.try_with(|ctx| (*ctx.borrow()).as_ref().map(|ctx| ctx.spawner.clone())) {
        Ok(spawner) => spawner,
        Err(_) => panic!("{}", crate::util::error::THREAD_LOCAL_DESTROYED_ERROR),
    }
}

/// Sets this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: Handle
pub(crate) fn enter(new: Handle) -> EnterGuard {
    scope!("context::EnterGuard::enter" =>
        match try_enter(new) {
            Some(guard) => guard,
            None => panic!("{}", crate::util::error::THREAD_LOCAL_DESTROYED_ERROR),
        }
    )
}

/// Sets this [`Handle`] as the current active [`Handle`].
///
/// [`Handle`]: Handle
pub(crate) fn try_enter(new: Handle) -> Option<EnterGuard> {
    CONTEXT
        .try_with(|ctx| {
            let old = ctx.borrow_mut().replace(new);
            EnterGuard(old)
        })
        .ok()
}

#[derive(Debug)]
pub(crate) struct EnterGuard(Option<Handle>);

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = self.0.take();
        });
    }
}
