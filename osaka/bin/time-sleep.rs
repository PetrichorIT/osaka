use std::cmp::Reverse;
use std::collections::BinaryHeap;

use osaka::{
    runtime::Runtime,
    sync::mpsc::channel,
    time::{sleep, SimTime},
    tprintln,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Event {
    time: SimTime,
    typ: usize, // 0 - wakeup, 1 - message
}

fn main() {
    // osaka::util::LOG_SCOPES.store(true, std::sync::atomic::Ordering::SeqCst);

    let rt = Runtime::new().unwrap();

    let (tx, mut rx) = channel::<usize>(32);

    let _handle = rt.spawn(async move {
        tprintln!("Started recv");
        while let Some(i) = rx.recv().await {
            tprintln!("Recived packet {}", i);
            sleep(1.0.into()).await;
            tprintln!("Sleep completed for {}", i);
        }
    });

    let mut events = BinaryHeap::<Reverse<Event>>::new();

    events.push(Reverse(Event {
        time: 1.0.into(),
        typ: 1,
    }));
    events.push(Reverse(Event {
        time: 5.0.into(),
        typ: 5,
    }));
    events.push(Reverse(Event {
        time: 10.0.into(),
        typ: 10,
    }));

    // This event may occur at the 10.5 minute mark
    // but will not be processed by the task until the
    // sleep has commenced thus until 11.0
    events.push(Reverse(Event {
        time: 10.5.into(),
        typ: 11,
    }));

    while let Some(Reverse(event)) = events.pop() {
        println!("SIM: {:?}", event);

        SimTime::set_now(event.time);
        rt.take_timestep();

        match event.typ {
            0 => {
                rt.poll_until_deadlock();
            }
            _ => {
                let my_tx = tx.clone();
                let _v = rt.block_on_or_deadlock(async move { my_tx.send(event.typ).await });
                println!("{:?}", _v);
                rt.poll_until_deadlock();
            }
        }

        // Run Runtime
        let next_wakeup = rt.next_wakeup();
        if let Some(next_wakeup) = next_wakeup {
            tprintln!("Inserting wakeup at: {}", next_wakeup);
            events.push(Reverse(Event {
                time: next_wakeup,
                typ: 0,
            }));
        }
    }

    drop(tx);
}
