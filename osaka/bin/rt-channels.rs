use osaka::{
    runtime::Runtime,
    sync::mpsc::{channel, unbounded_channel},
    tprintln,
};

fn main() {
    let rt = Runtime::new().expect("Failed to create runtime");

    // General setup
    // ===
    //
    // Tasks:
    // - Central Mangaer
    // - SubManagerAlpha
    // - SubManagerBeta
    //
    // Connections:
    // - upstream
    // - downstream
    // - main-alapha
    // - alpha-main
    // - main-beta

    let (up_tx, mut up_rx) = channel(32);
    let (down_tx, mut down_rx) = unbounded_channel();

    let (ma_tx, mut ma_rx) = channel(16);
    let (am_tx, mut am_rx) = channel(16);
    let (mb_tx, mut mb_rx) = channel(16);

    let _handle_main = rt.spawn(async move {
        while let Some(v) = up_rx.recv().await {
            tprintln!("[Main] Received value {} forwarding to Alpha", v);
            let v: usize = v;
            // Send it to alpha
            ma_tx.send(v).await.unwrap();
            // listen for result
            let result = am_rx.recv().await.unwrap();
            tprintln!("[Main] Received value {} forwarding to Beta / Down", result);
            // send to beta and down
            mb_tx.send(result).await.unwrap();
            down_tx.send(result).unwrap();
        }
    });

    let _handle_alpha = rt.spawn(async move {
        while let Some(v) = ma_rx.recv().await {
            tprintln!("[Alpha] Received value {} sending {}", v, v * 2);
            am_tx.send(v * 2).await.unwrap();
        }
    });

    let _handle_beta = rt.spawn(async move {
        while let Some(v) = mb_rx.recv().await {
            tprintln!("[Beta] Received value {}", v)
        }
    });

    for v in [1, 2, 3] {
        let my_up_tx = up_tx.clone();
        rt.block_on(async move {
            my_up_tx.send(v).await.unwrap();
        });

        rt.poll_until_deadlock();
        let mut results = Vec::new();
        while let Ok(v) = down_rx.try_recv() {
            results.push(v)
        }

        tprintln!("{:?}", results)
    }
}
