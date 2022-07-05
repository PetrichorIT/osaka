pub mod executor;
pub mod reactor;
pub mod task;
pub mod task_queue;
pub mod waker_util;

pub mod context;

pub mod driver;
pub mod handle;
pub mod spawner;

pub(crate) mod enter;
use std::future::Future;

pub(crate) use enter::enter;

use crate::{
    scope,
    time::{SimTime, TimeDriver},
};

use self::{
    handle::{EnterGuard, Handle},
    spawner::{BasicScheduler, BlockOnError},
    task::JoinHandle,
};

pub struct Runtime {
    scheduler: BasicScheduler,
    handle: Handle,
}

impl Runtime {
    pub fn new() -> std::io::Result<Runtime> {
        // use crate::runtime::{BasicScheduler, HandleInner, Kind};
        use crate::runtime::handle::HandleInner;

        // let (driver, resources) = driver::Driver::new(self.get_cfg())?;

        // Blocking pool
        // let blocking_pool = blocking::create_blocking_pool(self, self.max_blocking_threads);
        // let blocking_spawner = blocking_pool.spawner().clone();

        let handle_inner = HandleInner {
            // io_handle: resources.io_handle,
            // time_handle: resources.time_handle,
            // signal_handle: resources.signal_handle,
            // clock: resources.clock,
            // blocking_spawner,
        };

        // And now put a single-threaded scheduler on top of the timer. When
        // there are no futures ready to do something, it'll let the timer or
        // the reactor to generate some new stimuli for the futures to continue
        // in their life.
        let scheduler = BasicScheduler::new((), handle_inner, ());
        let spawner = scheduler.spawner().clone();

        Ok(Runtime {
            scheduler,
            handle: Handle { spawner },
        })
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.with_time(|| self.handle.spawn(future))
    }

    pub(crate) fn with_time<R>(&self, f: impl FnOnce() -> R) -> R {
        self.scheduler.with_time(f)
    }

    pub fn take_timestep(&self) {
        self.with_time(|| {
            TimeDriver::with_current(|mut c| c.take_timestep().into_iter().for_each(|w| w.wake()));
        })
    }

    pub fn next_wakeup(&self) -> Option<SimTime> {
        self.with_time(|| TimeDriver::with_current(|c| c.next_wakeup()))
    }

    pub fn poll_until_deadlock(&self) {
        scope!("Runtime::poll_until_deadlock" => {
            self.with_time(|| self.scheduler.poll_until_deadlock())
        })
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        scope!("Runtime::block_on" => {
            self.with_time(|| {
                let _enter = self.enter();
                self.scheduler.block_on(future).expect("'block_on' entcountered deadlock")
            })
        })
    }

    pub fn block_on_or_deadlock<F: Future>(&self, future: F) -> Result<F::Output, BlockOnError> {
        scope!("Runtime::block_on" => {
            self.with_time(|| {
                let _enter = self.enter();
                self.scheduler.block_on(future)
            })
        })
    }

    pub fn enter(&self) -> EnterGuard<'_> {
        scope!("Runtime::enter" => self.handle.enter())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // This ensures that tasks spawned on the basic runtime are dropped inside the
        // runtime's context.
        match self::context::try_enter(self.handle.clone()) {
            Some(guard) => self.scheduler.set_context_guard(guard),
            None => {
                // The context thread-local has already been destroyed.
                //
                // We don't set the guard in this case. Calls to osaka::spawn in task
                // destructors would fail regardless if this happens.
            }
        }
    }
}
