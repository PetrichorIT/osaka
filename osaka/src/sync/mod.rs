//! Synchronization primitives for use in asynchronous contexts.
//!
//! Osaka programs tend to be organized as a set of tasks where each task
//! operates independently and may be executed on separate physical threads. The
//! synchronization primitives provided in this module permit these independent
//! tasks to communicate together.
//!
//!
//! # Message passing
//!
//! The most common form of synchronization in a Osaka program is message
//! passing. Two tasks operate independently and send messages to each other to
//! synchronize. Doing so has the advantage of avoiding shared state.
//!
//! Message passing is implemented using channels. A channel supports sending a
//! message from one producer task to one or more consumer tasks. There are a
//! few flavors of channels provided by Osaka. Each channel flavor supports
//! different message passing patterns. When a channel supports multiple
//! producers, many separate tasks may **send** messages. When a channel
//! supports multiple consumers, many different separate tasks may **receive**
//! messages.
//!
//! Osaka provides many different channel flavors as different message passing
//! patterns are best handled with different implementations.
//!
//! ## `oneshot` channel
//!
//! The [`oneshot` channel][oneshot] supports sending a **single** value from a
//! single producer to a single consumer. This channel is usually used to send
//! the result of a computation to a waiter.
//!
//! **Example:** using a [`oneshot` channel][oneshot] to receive the result of a
//! computation.
//!
//! ```
//! use osaka::sync::oneshot;
//!
//! async fn some_computation() -> String {
//!     "represents the result of the computation".to_string()
//! }
//!
//! #[osaka::main]
//! async fn main() {
//!     let (tx, rx) = oneshot::channel();
//!
//!     osaka::spawn(async move {
//!         let res = some_computation().await;
//!         tx.send(res).unwrap();
//!     });
//!
//!     // Do other work while the computation is happening in the background
//!
//!     // Wait for the computation result
//!     let res = rx.await.unwrap();
//! }
//! ```
//!
//! Note, if the task produces a computation result as its final
//! action before terminating, the `JoinHandle` can be used to
//! receive that value instead of allocating resources for the
//! `oneshot` channel. Awaiting on `JoinHandle` returns `Result`. If
//! the task panics, the `Joinhandle` yields `Err` with the panic
//! cause.
//!
//! **Example:**
//!
//! ```
//! async fn some_computation() -> String {
//!     "the result of the computation".to_string()
//! }
//!
//! #[osaka::main]
//! async fn main() {
//!     let join_handle = osaka::spawn(async move {
//!         some_computation().await
//!     });
//!
//!     // Do other work while the computation is happening in the background
//!
//!     // Wait for the computation result
//!     let res = join_handle.await.unwrap();
//! }
//! ```
//!
//! [oneshot]: oneshot
//!
//! ## `mpsc` channel
//!
//! The [`mpsc` channel][mpsc] supports sending **many** values from **many**
//! producers to a single consumer. This channel is often used to send work to a
//! task or to receive the result of many computations.
//!
//! **Example:** using an mpsc to incrementally stream the results of a series
//! of computations.
//!
//! ```
//! use osaka::sync::mpsc;
//!
//! async fn some_computation(input: u32) -> String {
//!     format!("the result of computation {}", input)
//! }
//!
//! #[osaka::main]
//! async fn main() {
//!     let (tx, mut rx) = mpsc::channel(100);
//!
//!     osaka::spawn(async move {
//!         for i in 0..10 {
//!             let res = some_computation(i).await;
//!             tx.send(res).await.unwrap();
//!         }
//!     });
//!
//!     while let Some(res) = rx.recv().await {
//!         println!("got = {}", res);
//!     }
//! }
//! ```
//!
//! The argument to `mpsc::channel` is the channel capacity. This is the maximum
//! number of values that can be stored in the channel pending receipt at any
//! given time. Properly setting this value is key in implementing robust
//! programs as the channel capacity plays a critical part in handling back
//! pressure.
//!
//! A common concurrency pattern for resource management is to spawn a task
//! dedicated to managing that resource and using message passing between other
//! tasks to interact with the resource. The resource may be anything that may
//! not be concurrently used. Some examples include a socket and program state.
//! For example, if multiple tasks need to send data over a single socket, spawn
//! a task to manage the socket and use a channel to synchronize.
//!
//! **Example:** sending data from many tasks over a single socket using message
//! passing.
//!
//! Note that this example will fail since osaka does not provide net primitves
//! yet.
//!
//! ```ignore
//! use osaka::io::{self, AsyncWriteExt};
//! use osaka::net::TcpStream;
//! use osaka::sync::mpsc;
//!
//! #[osaka::main]
//! async fn main() -> io::Result<()> {
//!     let mut socket = TcpStream::connect("www.example.com:1234").await?;
//!     let (tx, mut rx) = mpsc::channel(100);
//!
//!     for _ in 0..10 {
//!         // Each task needs its own `tx` handle. This is done by cloning the
//!         // original handle.
//!         let tx = tx.clone();
//!
//!         osaka::spawn(async move {
//!             tx.send(&b"data to write"[..]).await.unwrap();
//!         });
//!     }
//!
//!     // The `rx` half of the channel returns `None` once **all** `tx` clones
//!     // drop. To ensure `None` is returned, drop the handle owned by the
//!     // current task. If this `tx` handle is not dropped, there will always
//!     // be a single outstanding `tx` handle.
//!     drop(tx);
//!
//!     while let Some(res) = rx.recv().await {
//!         socket.write_all(res).await?;
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! The [`mpsc`][mpsc] and [`oneshot`][oneshot] channels can be combined to
//! provide a request / response type synchronization pattern with a shared
//! resource. A task is spawned to synchronize a resource and waits on commands
//! received on a [`mpsc`][mpsc] channel. Each command includes a
//! [`oneshot`][oneshot] `Sender` on which the result of the command is sent.
//!
//! **Example:** use a task to synchronize a `u64` counter. Each task sends an
//! "fetch and increment" command. The counter value **before** the increment is
//! sent over the provided `oneshot` channel.
//!
//! ```ignore
//! use osaka::sync::{oneshot, mpsc};
//! use Command::Increment;
//!
//! enum Command {
//!     Increment,
//!     // Other commands can be added here
//! }
//!
//! #[osaka::main]
//! async fn main() {
//!     let (cmd_tx, mut cmd_rx) = mpsc::channel::<(Command, oneshot::Sender<u64>)>(100);
//!
//!     // Spawn a task to manage the counter
//!     osaka::spawn(async move {
//!         let mut counter: u64 = 0;
//!
//!         while let Some((cmd, response)) = cmd_rx.recv().await {
//!             match cmd {
//!                 Increment => {
//!                     let prev = counter;
//!                     counter += 1;
//!                     response.send(prev).unwrap();
//!                 }
//!             }
//!         }
//!     });
//!
//!     let mut join_handles = vec![];
//!
//!     // Spawn tasks that will send the increment command.
//!     for _ in 0..10 {
//!         let cmd_tx = cmd_tx.clone();
//!
//!         join_handles.push(osaka::spawn(async move {
//!             let (resp_tx, resp_rx) = oneshot::channel();
//!
//!             cmd_tx.send((Increment, resp_tx)).await.ok().unwrap();
//!             let res = resp_rx.await.unwrap();
//!
//!             println!("previous value = {}", res);
//!         }));
//!     }
//!
//!     // Wait for all tasks to complete
//!     for join_handle in join_handles.drain(..) {
//!         join_handle.await.unwrap();
//!     }
//! }
//! ```
//!
//! [mpsc]: mpsc
//!
//! ## `broadcast` channel
//!
//! The [`broadcast` channel] supports sending **many** values from
//! **many** producers to **many** consumers. Each consumer will receive
//! **each** value. This channel can be used to implement "fan out" style
//! patterns common with pub / sub or "chat" systems.
//!
//! This channel tends to be used less often than `oneshot` and `mpsc` but still
//! has its use cases.
//!
//! Basic usage
//!
//! ```
//! use osaka::sync::broadcast;
//!
//! #[osaka::main]
//! async fn main() {
//!     let (tx, mut rx1) = broadcast::channel(16);
//!     let mut rx2 = tx.subscribe();
//!
//!     osaka::spawn(async move {
//!         assert_eq!(rx1.recv().await.unwrap(), 10);
//!         assert_eq!(rx1.recv().await.unwrap(), 20);
//!     });
//!
//!     osaka::spawn(async move {
//!         assert_eq!(rx2.recv().await.unwrap(), 10);
//!         assert_eq!(rx2.recv().await.unwrap(), 20);
//!     });
//!
//!     tx.send(10).unwrap();
//!     tx.send(20).unwrap();
//! }
//! ```
//!
//! [`broadcast` channel]: crate::sync::broadcast
//!
//! ## `watch` channel
//!
//! The [`watch` channel] supports sending **many** values from a **single**
//! producer to **many** consumers. However, only the **most recent** value is
//! stored in the channel. Consumers are notified when a new value is sent, but
//! there is no guarantee that consumers will see **all** values.
//!
//! The [`watch` channel] is similar to a [`broadcast` channel] with capacity 1.
//!
//! Use cases for the [`watch` channel] include broadcasting configuration
//! changes or signalling program state changes, such as transitioning to
//! shutdown.
//!
//! **Example:** use a [`watch` channel] to notify tasks of configuration
//! changes. In this example, a configuration file is checked periodically. When
//! the file changes, the configuration changes are signalled to consumers.
//!
//!  *NOTE* NO time primitves yet.
//! ```ignore
//! use osaka::sync::watch;
//! use osaka::time::{self, Duration, Instant};
//!
//! use std::io;
//!
//! #[derive(Debug, Clone, Eq, PartialEq)]
//! struct Config {
//!     timeout: Duration,
//! }
//!
//! impl Config {
//!     async fn load_from_file() -> io::Result<Config> {
//!         // file loading and deserialization logic here
//! # Ok(Config { timeout: Duration::from_secs(1) })
//!     }
//! }
//!
//! async fn my_async_operation() {
//!     // Do something here
//! }
//!
//! #[osaka::main]
//! async fn main() {
//!     // Load initial configuration value
//!     let mut config = Config::load_from_file().await.unwrap();
//!
//!     // Create the watch channel, initialized with the loaded configuration
//!     let (tx, rx) = watch::channel(config.clone());
//!
//!     // Spawn a task to monitor the file.
//!     osaka::spawn(async move {
//!         loop {
//!             // Wait 10 seconds between checks
//!             time::sleep(Duration::from_secs(10)).await;
//!
//!             // Load the configuration file
//!             let new_config = Config::load_from_file().await.unwrap();
//!
//!             // If the configuration changed, send the new config value
//!             // on the watch channel.
//!             if new_config != config {
//!                 tx.send(new_config.clone()).unwrap();
//!                 config = new_config;
//!             }
//!         }
//!     });
//!
//!     let mut handles = vec![];
//!
//!     // Spawn tasks that runs the async operation for at most `timeout`. If
//!     // the timeout elapses, restart the operation.
//!     //
//!     // The task simultaneously watches the `Config` for changes. When the
//!     // timeout duration changes, the timeout is updated without restarting
//!     // the in-flight operation.
//!     for _ in 0..5 {
//!         // Clone a config watch handle for use in this task
//!         let mut rx = rx.clone();
//!
//!         let handle = osaka::spawn(async move {
//!             // Start the initial operation and pin the future to the stack.
//!             // Pinning to the stack is required to resume the operation
//!             // across multiple calls to `select!`
//!             let op = my_async_operation();
//!             osaka::pin!(op);
//!
//!             // Get the initial config value
//!             let mut conf = rx.borrow().clone();
//!
//!             let mut op_start = Instant::now();
//!             let sleep = time::sleep_until(op_start + conf.timeout);
//!             osaka::pin!(sleep);
//!
//!             loop {
//!                 osaka::select! {
//!                     _ = &mut sleep => {
//!                         // The operation elapsed. Restart it
//!                         op.set(my_async_operation());
//!
//!                         // Track the new start time
//!                         op_start = Instant::now();
//!
//!                         // Restart the timeout
//!                         sleep.set(time::sleep_until(op_start + conf.timeout));
//!                     }
//!                     _ = rx.changed() => {
//!                         conf = rx.borrow().clone();
//!
//!                         // The configuration has been updated. Update the
//!                         // `sleep` using the new `timeout` value.
//!                         sleep.as_mut().reset(op_start + conf.timeout);
//!                     }
//!                     _ = &mut op => {
//!                         // The operation completed!
//!                         return
//!                     }
//!                 }
//!             }
//!         });
//!
//!         handles.push(handle);
//!     }
//!
//!     for handle in handles.drain(..) {
//!         handle.await.unwrap();
//!     }
//! }
//! ```
//!
//! [`watch` channel]: mod@crate::sync::watch
//! [`broadcast` channel]: mod@crate::sync::broadcast
//!
//! # State synchronization
//!
//! The remaining synchronization primitives focus on synchronizing state.
//! These are asynchronous equivalents to versions provided by `std`. They
//! operate in a similar way as their `std` counterparts but will wait
//! asynchronously instead of blocking the thread.
//!
//! * [`Barrier`](Barrier) Ensures multiple tasks will wait for each other to
//!   reach a point in the program, before continuing execution all together.
//!
//! * [`Mutex`](Mutex) Mutual Exclusion mechanism, which ensures that at most
//!   one thread at a time is able to access some data.
//!
//! * [`Notify`](Notify) Basic task notification. `Notify` supports notifying a
//!   receiving task without sending data. In this case, the task wakes up and
//!   resumes processing.
//!
//! * [`RwLock`](RwLock) Provides a mutual exclusion mechanism which allows
//!   multiple readers at the same time, while allowing only one writer at a
//!   time. In some cases, this can be more efficient than a mutex.
//!
//! * [`Semaphore`](Semaphore) Limits the amount of concurrency. A semaphore
//!   holds a number of permits, which tasks may request in order to enter a
//!   critical section. Semaphores are useful for implementing limiting or
//!   bounding of any kind.

pub(crate) mod batch_semaphore;
pub use batch_semaphore::{AcquireError, TryAcquireError};

mod semaphore;
pub use semaphore::{OwnedSemaphorePermit, Semaphore, SemaphorePermit};

mod mutex;
pub use mutex::{MappedMutexGuard, Mutex, MutexGuard, OwnedMutexGuard, TryLockError};

mod rwlock;
pub use rwlock::owned_read_guard::OwnedRwLockReadGuard;
pub use rwlock::owned_write_guard::OwnedRwLockWriteGuard;
pub use rwlock::owned_write_guard_mapped::OwnedRwLockMappedWriteGuard;
pub use rwlock::read_guard::RwLockReadGuard;
pub use rwlock::write_guard::RwLockWriteGuard;
pub use rwlock::write_guard_mapped::RwLockMappedWriteGuard;
pub use rwlock::RwLock;

mod barrier;
pub use barrier::{Barrier, BarrierWaitResult};

pub(crate) mod notify;
pub use self::notify::Notify;

mod once_cell;
pub use self::once_cell::{OnceCell, SetError};

#[allow(dead_code)]
mod atomic_waker;
#[allow(unused)]
pub(crate) use atomic_waker::AtomicWaker;

pub mod broadcast;
pub mod mpsc;
pub mod oneshot;
pub mod watch;
