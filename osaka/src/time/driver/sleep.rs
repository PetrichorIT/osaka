use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::task::{Context, Poll};

use crate::time::{Duration, SimTime, TimeDriver};
use pin_project_lite::pin_project;

pub fn sleep_until(deadline: SimTime) -> Sleep {
    return Sleep::new_timeout(deadline);
}

pub fn sleep(duration: Duration) -> Sleep {
    match SimTime::now().checked_add(duration) {
        Some(deadline) => Sleep::new_timeout(deadline),
        None => Sleep::far_future(),
    }
}

pub static SLEEP_ID: AtomicUsize = AtomicUsize::new(0);

pin_project! {
    #[derive(Debug)]
    #[must_use = "Futures do nothing unless you `.await` or poll them"]
    pub struct Sleep {
        pub(crate) deadline: SimTime,
        pub(crate) id: usize,
    }
}

impl Sleep {
    pub fn new_timeout(deadline: SimTime) -> Sleep {
        let next = SLEEP_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Sleep { deadline, id: next }
    }

    pub fn far_future() -> Sleep {
        Self::new_timeout(SimTime::MAX)
    }

    pub fn deadline(&self) -> SimTime {
        self.deadline
    }

    pub fn is_elapsed(&self) -> bool {
        self.deadline <= SimTime::now()
    }

    /// Resets the `Sleep` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Sleep`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    ///
    /// To call this method, you will usually combine the call with
    /// [`Pin::as_mut`], which lets you call the method without consuming the
    /// `Sleep` itself.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use tokio::time::{Duration, Instant};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let sleep = tokio::time::sleep(Duration::from_millis(10));
    /// tokio::pin!(sleep);
    ///
    /// sleep.as_mut().reset(Instant::now() + Duration::from_millis(20));
    /// # }
    /// ```
    ///
    /// See also the top-level examples.
    ///
    /// [`Pin::as_mut`]: fn@std::pin::Pin::as_mut
    pub fn reset(self: Pin<&mut Self>, deadline: SimTime) {
        self.reset_inner(deadline)
    }

    fn reset_inner(self: Pin<&mut Self>, deadline: SimTime) {
        let me = self.project();
        // Reogranize timer calls.
        TimeDriver::with_current(|mut driver| driver.reset_sleeper(*me.id, deadline));
        *me.deadline = deadline;
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.deadline.cmp(&SimTime::now()) {
            Ordering::Greater => {
                // Setup waker
                TimeDriver::with_current(|mut driver| driver.wake_sleeper(&self, cx));
                Poll::Pending
            }
            Ordering::Equal => Poll::Ready(()),
            Ordering::Less => Poll::Ready(()),
        }
    }
}
