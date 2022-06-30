use std::cmp::Ordering;
use std::future::Future;
use std::sync::atomic::AtomicUsize;
use std::task::{Context, Poll};

use crate::time::{Duration, SimTime};
use crate::tprintln;

use super::TIME_DRIVER;

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

#[must_use = "Futures do nothing unless you `.await` or poll them"]
pub struct Sleep {
    pub(crate) deadline: SimTime,
    pub(crate) id: usize,
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
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.deadline.cmp(&SimTime::now()) {
            Ordering::Greater => {
                //
                TIME_DRIVER.with(|b| b.borrow_mut().wake_sleeper(&self, cx));
                Poll::Pending
            }
            Ordering::Equal => Poll::Ready(()),
            Ordering::Less => {
                tprintln!(
                    "[{}] Tried to resolve sleep: {}",
                    SimTime::now(),
                    self.deadline
                );
                Poll::Ready(())
            }
        }
    }
}
