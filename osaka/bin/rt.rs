use osaka::{runtime::*, sync::Semaphore, tprintln, util::LOG_SCOPES};
use std::sync::atomic::Ordering;

fn main() {
    LOG_SCOPES.store(true, Ordering::SeqCst);

    let exec = Runtime::new().unwrap();
    let semaphore = Semaphore::new(3);

    let c = 4;

    let handle = exec.spawn(async move {
        tprintln!("[1] Evaluated");
        42
    });

    let h2 = exec.spawn(async move {
        tprintln!("[2] Pre Await");
        let val = handle.await.unwrap();
        tprintln!("[2] Post Await");

        let permit = semaphore.acquire_many(c).await.unwrap();
        tprintln!("[2] Post Semaphore");
        drop(permit);
        val
    });

    let _v = exec.poll_until_deadlock();
    if c <= 3 {
        assert_eq!(exec.block_on(h2).unwrap(), 42)
    } else {
        assert!(exec.block_on_or_deadlock(h2).is_err())
    };
}
