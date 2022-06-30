use std::collections::BinaryHeap;

use osaka::{
    runtime::Runtime,
    sync::mpsc::channel,
    time::{sleep, SimTime, TimeDriver},
    tprintln,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Event {
    time: SimTime,
    typ: usize, // 0 - wakeup, 1 - message
}

#[test]
fn main() {
    let rt = Runtime::new().unwrap();

    let (tx, mut rx) = channel::<usize>(32);

    let handle = rt.spawn(async move {
        tprintln!("Started recv");
        while let Some(i) = rx.recv().await {
            tprintln!("[{}] Recived packet {}", SimTime::now(), i);
            sleep(1.0.into()).await;
        }
    });

    let mut events = BinaryHeap::<Event>::new();

    events.push(Event {
        time: 1.0.into(),
        typ: 1,
    });
    events.push(Event {
        time: 5.0.into(),
        typ: 5,
    });
    events.push(Event {
        time: 10.0.into(),
        typ: 10,
    });

    while let Some(event) = events.pop() {
        SimTime::set_now(event.time);
        TimeDriver::with_current(|mut c| c.take_timestep().into_iter().for_each(|w| w.wake()));

        match event.typ {
            0 => {}
            _ => rt.block_on(tx.send(event.typ)).unwrap(),
        }

        // Run Runtime
        let next_wakeup = TimeDriver::with_current(|c| c.next_wakeup());
        if let Some(next_wakeup) = next_wakeup {
            tprintln!("Inserting wakeup at: {}", next_wakeup);
            events.push(Event {
                time: next_wakeup,
                typ: 0,
            });
        }
    }

    drop(tx);
    rt.block_on(handle).unwrap();

    panic!("")
}
