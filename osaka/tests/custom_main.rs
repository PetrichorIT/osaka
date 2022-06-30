use osaka::{runtime::Runtime, sync::Notify, tprintln};
use std::{io::Result, sync::Arc};

#[test]
fn main() -> Result<()> {
    // executor::block_on(async_main())

    let rt = Runtime::new()?;
    let notify = Arc::new(Notify::new());
    let notify2 = notify.clone();

    let h1 = rt.spawn(async move {
        notify.notified().await;
        tprintln!("Task 1 Executed");
        42
    });

    let h2 = rt.spawn(async move {
        tprintln!("Task 2 Executed");
        notify2.notify_waiters();
        69
    });

    let r1 = rt.block_on(h1).unwrap();
    let r2 = rt.block_on(h2).unwrap();

    assert_eq!(r1, 42);
    assert_eq!(r2, 69);

    // panic!("")
    Ok(())
}
