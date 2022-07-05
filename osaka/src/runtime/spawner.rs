use super::context::EnterGuard;
use super::handle::HandleInner;
use super::task::{self, OwnedTasks, Schedule};
use crate::sync::notify::Notify;
use crate::time::TimeDriver;
use crate::util::atomic_cell::AtomicCell;
use crate::util::{poll_fn, waker_ref, Wake, WakerRef};
use crate::{adapter::Mutex, scoped_thread_local, task::JoinHandle};
use crate::{pin, scope, tprintln};
use core::panic;
use std::sync::atomic::{AtomicBool, Ordering::*};
use std::task::Poll::*;
use std::{cell::RefCell, collections::VecDeque, fmt, future::Future, sync::Arc};

/// Executes tasks on the current thread
pub(crate) struct BasicScheduler {
    core: AtomicCell<Core>,

    time_driver: AtomicCell<TimeDriver>,

    /// Notifier for waking up other threads to steal the
    /// driver.
    notify: Notify,

    /// Sendable task spawner
    spawner: Spawner,

    /// This is usually None, but right before dropping the BasicScheduler, it
    /// is changed to `Some` with the context being the runtime's own context.
    /// This ensures that any tasks dropped in the `BasicScheduler`s destructor
    /// run in that runtime's context.
    context_guard: Option<EnterGuard>,
}

const INITIAL_CAPACITY: usize = 64;

impl BasicScheduler {
    pub(crate) fn new(_driver: (), handle_inner: HandleInner, _config: ()) -> BasicScheduler {
        let spawner = Spawner {
            shared: Arc::new(Shared {
                queue: Mutex::new(Some(VecDeque::with_capacity(INITIAL_CAPACITY))),
                owned: OwnedTasks::new(),
                woken: AtomicBool::new(false),
                handle_inner,
            }),
        };

        let core = AtomicCell::new(Some(Box::new(Core {
            tasks: VecDeque::with_capacity(INITIAL_CAPACITY),
            spawner: spawner.clone(),
            tick: 0,
            unhandled_panic: false,
        })));

        BasicScheduler {
            core,
            time_driver: AtomicCell::new(Some(Box::new(TimeDriver::new()))),
            notify: Notify::new(),
            spawner,
            context_guard: None,
        }
    }

    pub(crate) fn with_time<R>(&self, f: impl FnOnce() -> R) -> R {
        if let Some(out_driver) = self.time_driver.take() {
            let other_driver = TimeDriver::swap_context(out_driver);

            let ret = f();

            if let Some(other_driver) = other_driver {
                let our_driver =
                    TimeDriver::swap_context(other_driver).expect("Somebody stole our driver");
                self.time_driver.set(our_driver);
            } else {
                let our_driver = TimeDriver::unset_context();
                self.time_driver.set(our_driver);
            }
            ret
        } else {
            if TimeDriver::is_context_set() {
                tprintln!("Warning: Reusing time driver allready in context, missing own driver.");
                f()
            } else {
                panic!("Not time driver found")
            }
        }
    }

    pub(crate) fn spawner(&self) -> &Spawner {
        &self.spawner
    }

    pub(crate) fn poll_until_deadlock(&self) {
        scope!("BasicScheduler::poll_until_deadlock" => {
            loop {
                if let Some(core) = self.take_core() {
                    return core.poll_until_deadlock()
                } else {
                    panic!("HUH")
                }
            }
        })
    }

    #[track_caller]
    pub(crate) fn block_on<F: Future>(&self, future: F) -> Result<F::Output, BlockOnError> {
        scope!("BasicScheduler::block_on" => {
            pin!(future);

            // Attempt to steal the scheduler core and block_on the future if we can
            // there, otherwise, lets select on a notification that the core is
            // available or the future is complete.
            loop {
                if let Some(core) = self.take_core() {
                    return core.block_on(future);
                } else {
                    let mut enter = crate::runtime::enter(false);

                    let notified = self.notify.notified();
                    pin!(notified);

                    if let Some(out) = enter
                        .block_on(poll_fn(|cx| {
                            if notified.as_mut().poll(cx).is_ready() {
                                return Ready(None);
                            }

                            if let Ready(out) = future.as_mut().poll(cx) {
                                return Ready(Some(out));
                            }

                            Pending
                        }))
                        .expect("Failed to `Enter::block_on`")
                    {
                        return Ok(out);
                    }
                }
            }
        })
    }

    fn take_core(&self) -> Option<CoreGuard<'_>> {
        let core = self.core.take()?;

        Some(CoreGuard {
            context: Context {
                spawner: self.spawner.clone(),
                core: RefCell::new(Some(core)),
            },
            basic_scheduler: self,
        })
    }

    pub(super) fn set_context_guard(&mut self, guard: EnterGuard) {
        self.context_guard = Some(guard);
    }
}

impl Drop for BasicScheduler {
    fn drop(&mut self) {
        scope!("BasicScheduler::drop" => {
            // Avoid a double panic if we are currently panicking and
            // the lock may be poisoned.

            let core = match self.take_core() {
                Some(core) => core,
                None if std::thread::panicking() => return,
                None => panic!("Oh no! We never placed the Core back, this is a bug!"),
            };

            core.enter(|mut core, context| {
                // Drain the OwnedTasks collection. This call also closes the
                // collection, ensuring that no tasks are ever pushed after this
                // call returns.
                context.spawner.shared.owned.close_and_shutdown_all();

                // Drain local queue
                // We already shut down every task, so we just need to drop the task.
                while let Some(task) = core.pop_task() {
                    drop(task);
                }

                // Drain remote queue and set it to None
                let remote_queue = core.spawner.shared.queue.lock().take();

                // Using `Option::take` to replace the shared queue with `None`.
                // We already shut down every task, so we just need to drop the task.
                if let Some(remote_queue) = remote_queue {
                    for task in remote_queue {
                        drop(task);
                    }
                }

                assert!(context.spawner.shared.owned.is_empty());

                // Submit metrics
                // core.metrics.submit(&core.spawner.shared.worker_metrics);

                (core, ())
            });
        })
    }
}

impl fmt::Debug for BasicScheduler {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BasicScheduler").finish()
    }
}

#[derive(Debug, Clone)]
pub struct Spawner {
    shared: Arc<Shared>,
}

impl Spawner {
    pub(crate) fn spawn<F>(&self, future: F, id: super::task::Id) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (handle, notified) = self.shared.owned.bind(future, self.shared.clone(), id);

        if let Some(notified) = notified {
            self.shared.schedule(notified);
        }

        handle
    }

    fn pop(&self) -> Option<task::Notified> {
        match self.shared.queue.lock().as_mut() {
            Some(queue) => queue.pop_front(),
            None => None,
        }
    }

    fn waker_ref(&self) -> WakerRef<'_> {
        // Set woken to true when enter block_on, ensure outer future
        // be polled for the first time when enter loop
        self.shared.woken.store(true, Release);
        waker_ref(&self.shared)
    }

    // reset woken to false and return original value
    pub(crate) fn reset_woken(&self) -> bool {
        self.shared.woken.swap(false, AcqRel)
    }

    #[allow(dead_code)]
    pub(crate) fn as_handle_inner(&self) -> &HandleInner {
        &self.shared.handle_inner
    }
}

#[derive(Debug)]
pub(crate) struct Shared {
    queue: Mutex<Option<VecDeque<task::Notified>>>,
    owned: OwnedTasks,
    woken: AtomicBool,
    handle_inner: HandleInner,
}

impl Wake for Shared {
    fn wake(arc_self: Arc<Self>) {
        Wake::wake_by_ref(&arc_self)
    }

    /// Wake by reference
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.woken.store(true, Release);
        // arc_self.unpark.unpark();
    }
}

impl Schedule for Arc<Shared> {
    fn release(&self, task: &super::task::Task) -> Option<super::task::Task> {
        self.owned.remove(task)
    }

    fn schedule(&self, task: super::task::Notified) {
        CURRENT.with(|maybe_cx| match maybe_cx {
            Some(cx) if Arc::ptr_eq(self, &cx.spawner.shared) => {
                let mut core = cx.core.borrow_mut();

                // If `None`, the runtime is shutting down, so there is no need
                // to schedule the task.
                if let Some(core) = core.as_mut() {
                    core.push_task(task);
                }
            }
            _ => {
                // unimplemented!("We wont park or unpark any threads")
                // Track that a task was scheduled from **outside** of the runtime.
                // self.scheduler_metrics.inc_remote_schedule_count();

                // If the queue is None, then the runtime has shut down. We
                // don't need to do anything with the notification in that case.
                let mut guard = self.queue.lock();
                if let Some(queue) = guard.as_mut() {
                    queue.push_back(task);
                    drop(guard);
                    //     self.unpark.unpark();
                }
            }
        });
    }
}

/// Thread-local context.
struct Context {
    /// Handle to the spawner
    spawner: Spawner,

    /// Scheduler core, enabling the holder of `Context` to execute the
    /// scheduler.
    core: RefCell<Option<Box<Core>>>,
}

// Tracks the current BasicScheduler.
scoped_thread_local!(static CURRENT: Context);

impl Context {
    /// Execute the closure with the given scheduler core stored in the
    /// thread-local context.
    fn run_task<R>(&self, core: Box<Core>, f: impl FnOnce() -> R) -> (Box<Core>, R) {
        // core.metrics.incr_poll_count();
        self.enter(core, || f() /* crate::coop::budget(f) */)
    }

    /// Blocks the current thread until an event is received by the driver,
    /// including I/O events, timer events, ...
    #[allow(dead_code)]
    fn park(&self, _core: Box<Core>) -> Box<Core> {
        unimplemented!("We wont park or unpark any threads")
    }

    /// Checks the driver for new events without blocking the thread.
    fn park_yield(&self, _core: Box<Core>) -> Box<Core> {
        unimplemented!("We wont park or unpark any threads")
    }

    fn enter<R>(&self, core: Box<Core>, f: impl FnOnce() -> R) -> (Box<Core>, R) {
        scope!("Context::enter" => {
                // Store the scheduler core in the thread-local context
                //
                // A drop-guard is employed at a higher level.
                *self.core.borrow_mut() = Some(core);

                // Execute the closure while tracking the execution budget
                let ret = f();

                // Take the scheduler core back
                let core = self.core.borrow_mut().take().expect("core missing");
                (core, ret)
        })
    }
}

struct Core {
    /// Scheduler run queue
    tasks: VecDeque<task::Notified>,

    /// Sendable task spawner
    spawner: Spawner,

    /// Current tick
    tick: u32,

    // /// Runtime driver
    // ///
    // /// The driver is removed before starting to park the thread
    // driver: Option<Driver>,

    // /// Metrics batch
    // metrics: MetricsBatch,
    /// True if a task panicked without being handled and the runtime is
    /// configured to shutdown on unhandled panic.
    unhandled_panic: bool,
}

impl Core {
    fn pop_task(&mut self) -> Option<task::Notified> {
        self.tasks.pop_front()
    }

    fn push_task(&mut self, task: task::Notified) {
        self.tasks.push_back(task);
    }
}

struct CoreGuard<'a> {
    context: Context,
    basic_scheduler: &'a BasicScheduler,
}

impl CoreGuard<'_> {
    ///
    /// Consider this function to behave similar
    /// to 'block_on' where the future to block on
    /// dependes on all available tasks in the runtime.
    ///
    /// Thereby this function may either succeed with executing
    /// this imaginary future, or may fall into a deadlock thus
    /// returning. Since the completion of the imaginary future
    /// implies that all tasks are resolved, this can be modelled
    /// as a deadlock, by defining a deadlock as the inability to
    /// make further progress.
    ///
    /// Thereby this function returns once the scheduler queue has
    /// been emptied.
    ///
    /// Note that this function also returns when encountering an
    /// unhandled panic. This however is not yet indicated by the return
    /// type and this is only indicated in the core.
    ///
    #[track_caller]
    fn poll_until_deadlock(self) {
        scope!("CoreGuard::poll_until_deadlock" => {
            self.enter(|mut core, context| {
                let _enter = crate::runtime::enter(false);

                // Future must not be pinned since there is no future

                'outer: loop {
                    // Progress counter
                    let mut c = 0;
                    for _i in 0..10 {
                        // Make sure we didn't hit an unhandled_panic
                        if core.unhandled_panic {
                            return (core, ());
                        }

                        // Get and increment the current tick
                        let tick = core.tick;
                        core.tick = core.tick.wrapping_add(1);

                        // if tick % core.spawner.shared.config.global_queue_interval == 0
                        let entry = if tick % 5 == 0 {
                            core.spawner.pop().or_else(|| core.tasks.pop_front())
                        } else {
                            core.tasks.pop_front().or_else(|| core.spawner.pop())
                        };

                        let task = match entry {
                            Some(entry) => entry,
                            None => {
                                // If `entry` is None, the core queue and the spawner queue
                                // is empty. Since only those queues exists in a single threaded
                                // runtime, and time sensitive calls to [Waker] are occure
                                // only by using another [CoreGuard] from a [Runtime]
                                // this empty status is guaranteed to remaing until this call ends.

                                // As told this is very likly a deadlock
                                // However make another iteration if work was done in this current
                                // iteration, to perpare for later edge cases and to handle
                                // unhandled panics
                                if c == 0  {
                                    return (core, ());
                                } else {
                                    continue 'outer;
                                }
                            }
                        };

                        let task = context.spawner.shared.owned.assert_owner(task);

                        let (c, _) = scope!("CoreGuard::block_on::run_task" => context.run_task(core, || {
                            c += 1;
                            task.run();
                        }));


                        core = c;
                    }
                }
            });
        })
    }

    #[track_caller]
    fn block_on<F: Future>(self, future: F) -> Result<F::Output, BlockOnError> {
        scope!("CoreGuard::block_on" => {
            let ret = self.enter(|mut core, context| {
                    let _enter = crate::runtime::enter(false);
                    let waker = context.spawner.waker_ref();
                    let mut cx = std::task::Context::from_waker(&waker);

                    pin!(future);

                    'outer: loop {
                        // Progress count
                        let mut c = 0;

                        if core.spawner.reset_woken() {
                            let (c, res) = scope!("CoreGuard::block_on::reset_woken_poll" => {
                                c += 1;
                                context.enter(core, || future.as_mut().poll(&mut cx))
                            });

                            core = c;

                            if let Ready(v) = res {
                                return (core, Some(v));
                            }
                        }

                        //for _ in 0..core.spawner.shared.config.event_interval

                        let i_max = 10;
                        for _i in 0..i_max {
                            // Make sure we didn't hit an unhandled_panic
                            if core.unhandled_panic {
                                return (core, None);
                            }

                            // Get and increment the current tick
                            let tick = core.tick;
                            core.tick = core.tick.wrapping_add(1);

                            // if tick % core.spawner.shared.config.global_queue_interval == 0
                            let entry = if tick % 5 == 0 {
                                core.spawner.pop().or_else(|| core.tasks.pop_front())
                            } else {
                                core.tasks.pop_front().or_else(|| core.spawner.pop())
                            };

                            let task = match entry {
                                Some(entry) => entry,
                                None => {
                                    // TODO does that fix it ?
                                    // core = context.park(core);

                                    // This should act as a deadlock detection.
                                    if c == 0  {
                                        return (core, None);
                                    } else {
                                        continue 'outer;
                                    }


                                    // Try polling the `block_on` future next
                                    // continue 'outer;
                                }
                            };

                            let task = context.spawner.shared.owned.assert_owner(task);

                            let (c, _) = scope!("CoreGuard::block_on::run_task" => context.run_task(core, || {
                                c += 1;
                                task.run();
                            }));


                            core = c;
                        }

                        // Yield to the driver, this drives the timer and pulls any
                        // pending I/O events.
                        core = context.park_yield(core);
                    }
                });

            match ret {
                Some(ret) => Ok(ret),
                None => {
                    // `block_on` panicked.
                    return Err(BlockOnError::Deadlock);
                    // panic!("a spawned task panicked and the runtime is configured to shut down on unhandled panic");
                }
            }
        })
    }

    /// Enters the scheduler context. This sets the queue and other necessary
    /// scheduler state in the thread-local.
    fn enter<F, R>(self, f: F) -> R
    where
        F: FnOnce(Box<Core>, &Context) -> (Box<Core>, R),
    {
        scope!("CoreGuard::enter" => {
            // Remove `core` from `context` to pass into the closure.
            let core = self.context.core.borrow_mut().take().expect("core missing");

            // Call the closure and place `core` back
            let (core, ret) = CURRENT.set(&self.context, || f(core, &self.context));

            *self.context.core.borrow_mut() = Some(core);

            ret
        })
    }
}

impl Drop for CoreGuard<'_> {
    fn drop(&mut self) {
        if let Some(core) = self.context.core.borrow_mut().take() {
            // Replace old scheduler back into the state to allow
            // other threads to pick it up and drive it.
            self.basic_scheduler.core.set(core);

            // Wake up other possible threads that could steal the driver.
            self.basic_scheduler.notify.notify_one()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BlockOnError {
    Deadlock,
}

impl std::fmt::Display for BlockOnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockOnError::Deadlock")
    }
}
