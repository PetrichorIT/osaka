use osaka::{
    runtime::Runtime,
    time::{interval, Duration, SimTime},
    tprintln,
};

fn main() {
    test_burst();
    test_delay();
    test_skip();
}

fn test_burst() {
    let rt = Runtime::new().unwrap();
    SimTime::set_now(SimTime::MIN);

    rt.spawn(async move {
        let mut interval = interval(3.0.into());

        // Ticks will com at
        // 0, 3, 6, 9, 12, 15, 18 ...

        // Each of the first 4 ticks will be delay the next
        // 2, 3, 4, 5

        // Thus expected:
        // Tick #1: Await 0s Resolve 0s TimeJump to 2s
        // Tick #2: Await 2s Resolve 3s TimeJump to 6s
        // Tick #3: Await 6s Resolve 6s TimeJump to 10s
        // Tick #4: Await 10s Resolve 10s TimeJump to 15s
        //   -> This tick could have occured at 9s but it was blocked,
        //      nonetheless this works fine sofar
        // End of loop
        // Tick #5: Await 15s Resolve 15s (scheduled for 12s)
        // Tick #6: Await 15s Resolve 15s (scheduled for 15s)
        // Tick #7: Await 15s Resolve 18s (scheduled for 18s)
        for i in 2..=5 {
            // Expects to be called every 3 seconds
            tprintln!("[{}] Awaiting tick", i);
            interval.tick().await;
            tprintln!("[{}] Got tick", i);

            let current = SimTime::now();
            let wait = current + Duration::new(i, 0);
            SimTime::set_now(wait);
            tprintln!("[{}] Forwarded time (eq. sleep(i as Seconds))", i)
        }

        tprintln!("[6] Awaiting tick");
        assert_eq!(SimTime::now(), 15.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 15.0);
        tprintln!("[6] Got tick");

        tprintln!("[7] Awaiting tick");
        assert_eq!(SimTime::now(), 15.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 15.0);
        tprintln!("[7] Got tick");

        tprintln!("[8] Awaiting tick");
        assert_eq!(SimTime::now(), 15.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 18.0);
        tprintln!("[8] Got tick");
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

fn test_delay() {
    let rt = Runtime::new().unwrap();
    SimTime::set_now(SimTime::MIN);

    rt.spawn(async move {
        let mut interval = interval(3.0.into());
        interval.set_missed_tick_behavior(osaka::time::MissedTickBehavior::Delay);

        // Ticks will com at
        // 0, 3, 6, 9, 12, 15, 18 ...

        // Each of the first 4 ticks will be delay the next
        // 2, 3, 4, 5

        // Thus expected:
        // Tick #1: Await 0s Resolve 0s TimeJump to 2s
        // Tick #2: Await 2s Resolve 3s TimeJump to 6s
        // Tick #3: Await 6s Resolve 6s TimeJump to 10s
        // Tick #4: Await 10s Resolve 10s TimeJump to 15s
        //   -> This tick could have occured at 9s but it was blocked,
        //      nonetheless this works fine sofar
        // End of loop
        // Tick #5: Await 15s Resolve 15s (scheduled for 12s)
        // Tick #6: Await 15s Resolve 18s (scheduled for 15s, got +3 delay)
        // Tick #7: Await 18s Resolve 21s (scheduled for 18s)
        for i in 2..=5 {
            // Expects to be called every 3 seconds
            tprintln!("[{}] Awaiting tick", i);
            interval.tick().await;
            tprintln!("[{}] Got tick", i);

            let current = SimTime::now();
            let wait = current + Duration::new(i, 0);
            SimTime::set_now(wait);
            tprintln!("[{}] Forwarded time (eq. sleep(i as Seconds))", i)
        }

        tprintln!("[6] Awaiting tick");
        assert_eq!(SimTime::now(), 15.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 15.0);
        tprintln!("[6] Got tick");

        tprintln!("[7] Awaiting tick");
        assert_eq!(SimTime::now(), 15.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 18.0);
        tprintln!("[7] Got tick");

        tprintln!("[8] Awaiting tick");
        assert_eq!(SimTime::now(), 18.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 21.0);
        tprintln!("[8] Got tick");
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

fn test_skip() {
    let rt = Runtime::new().unwrap();
    SimTime::set_now(SimTime::MIN);

    rt.spawn(async move {
        let mut interval = interval(3.0.into());
        interval.set_missed_tick_behavior(osaka::time::MissedTickBehavior::Skip);

        // Ticks will com at
        // 0, 3, 6, 9, 12, 15, 18 ...

        // Each of the first 4 ticks will be delay the next
        // 2, 3, 4, 5

        // Thus expected:
        // Tick #1: Await 0s Resolve 0s TimeJump to 2s
        // Tick #2: Await 2s Resolve 3s TimeJump to 6s
        // Tick #3: Await 6s Resolve 6s TimeJump to 10s
        // Tick #4: Await 10s Resolve 10s TimeJump to 15s
        //   -> This tick could have occured at 9s but it was blocked,
        //      nonetheless this works fine sofar
        // End of loop
        // Wait 1s to misalign
        // Tick #5: Await 16s Resolve 16s (scheduled for 12s)
        // Tick #6: Await 16s Resolve 18s (scheduled for 15s, got +3 delay)
        // Tick #7: Await 18s Resolve 21s (scheduled for 18s)
        for i in 2..=5 {
            // Expects to be called every 3 seconds
            tprintln!("[{}] Awaiting tick", i);
            interval.tick().await;
            tprintln!("[{}] Got tick", i);

            let current = SimTime::now();
            let wait = current + Duration::new(i, 0);
            SimTime::set_now(wait);
            tprintln!("[{}] Forwarded time (eq. sleep(i as Seconds))", i)
        }

        // Since t = 15s and 15s is perfectly aligned
        // this would behave identifcally to Delay
        // thus misallign the simtime with purpose
        SimTime::set_now(SimTime::now() + Duration::new(1, 0));
        tprintln!("[*] Missaligning time");

        tprintln!("[6] Awaiting tick");
        assert_eq!(SimTime::now(), 16.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 16.0);
        tprintln!("[6] Got tick");

        tprintln!("[7] Awaiting tick");
        assert_eq!(SimTime::now(), 16.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 18.0);
        tprintln!("[7] Got tick");

        tprintln!("[8] Awaiting tick");
        assert_eq!(SimTime::now(), 18.0);
        interval.tick().await;
        assert_eq!(SimTime::now(), 21.0);
        tprintln!("[8] Got tick");
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
