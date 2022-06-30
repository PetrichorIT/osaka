use crate::tprintln;

use super::SimTime;
use std::cell::{RefCell, RefMut};
use std::collections::BinaryHeap;
use std::task::{Context, Waker};

mod sleep;
pub use sleep::*;

thread_local! {
    pub static TIME_DRIVER: RefCell<TimeDriver> = RefCell::new(TimeDriver::new());
}

#[derive(Debug, Default)]
pub struct TimeDriver {
    sleeps: BinaryHeap<Entry>,
}

impl TimeDriver {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn wake_sleeper(&mut self, source: &Sleep, cx: &mut Context) {
        assert!(source.deadline >= SimTime::now());

        if self.sleeps.iter().any(|e| e.id == source.id) {
            return;
        }

        tprintln!("Registering sleeper");
        self.sleeps.push(Entry {
            deadline: source.deadline,
            id: source.id,
            waker: cx.waker().clone(),
        })
    }

    pub fn with_current<R>(f: impl FnOnce(RefMut<'_, Self>) -> R) -> R {
        TIME_DRIVER.with(|c| f(c.borrow_mut()))
    }

    pub fn next_wakeup(&self) -> Option<SimTime> {
        self.sleeps.peek().map(|v| v.deadline)
    }

    pub fn take_timestep(&mut self) -> Vec<Waker> {
        let mut wakers = Vec::new();

        while let Some(peeked) = self.sleeps.peek() {
            if peeked.deadline == SimTime::now() {
                wakers.push(self.sleeps.pop().unwrap().waker)
            } else {
                break;
            }
        }

        wakers
    }
}

#[derive(Debug)]
struct Entry {
    deadline: SimTime,
    waker: Waker,
    id: usize,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.id == other.id
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.id == other.id {
            true => std::cmp::Ordering::Equal,
            _ => self.deadline.cmp(&other.deadline),
        }
    }
}
