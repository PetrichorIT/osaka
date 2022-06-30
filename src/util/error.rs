/// Error string explaining that the Osakacontext hasn't been instantiated.
pub(crate) const CONTEXT_MISSING_ERROR: &str =
    "there is no reactor running, must be called from the context of a Osaka1.x runtime";

// some combinations of features might not use this
#[allow(dead_code)]
/// Error string explaining that the Osakacontext is shutting down and cannot drive timers.
pub(crate) const RUNTIME_SHUTTING_DOWN_ERROR: &str =
    "A Osaka1.x context was found, but it is being shutdown.";

// some combinations of features might not use this
#[allow(dead_code)]
/// Error string explaining that the Osakacontext is not available because the
/// thread-local storing it has been destroyed. This usually only happens during
/// destructors of other thread-locals.
pub(crate) const THREAD_LOCAL_DESTROYED_ERROR: &str =
    "The Osakacontext thread-local variable has been destroyed.";
