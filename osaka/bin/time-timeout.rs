use osaka::{
    runtime::Runtime,
    time::{sleep, timeout, SimTime},
    tprintln,
};

fn main() {
    let rt = Runtime::new().unwrap();

    rt.spawn(async move {
        // check the first timeout
        // sheduled at 0s
        let tout = timeout(10.0.into(), sleep(15.0.into()));
        let r = tout.await;
        tprintln!("Resolved first timeout to {:?}", r);

        let tout = timeout(10.0.into(), sleep(5.0.into()));
        let r = tout.await;
        tprintln!("Resolved second timeout to {:?}", r);
    });

    loop {
        rt.take_timestep();
        rt.poll_until_deadlock();
        if let Some(wkp) = rt.next_wakeup() {
            SimTime::set_now(wkp)
        } else {
            break;
        }
    }
}
