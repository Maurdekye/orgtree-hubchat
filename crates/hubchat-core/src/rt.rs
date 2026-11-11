//! The seam between the core and what it runs on (Hubchat Web, spec R1.1).
//!
//! The core is written against tokio: tasks, timers, a clock, files. In the
//! apps that is exactly what it gets: on every native target each name here
//! IS the tokio or std item it stands for, so nothing about the apps changes.
//! In a browser (`wasm32-unknown-unknown`) there is no tokio runtime, no
//! thread, no `std` clock and no file system: the same names are the
//! browser's task queue, its timers and its clocks. Core code uses these
//! names and never `tokio::spawn`, `tokio::task`, `tokio::time`, `tokio::fs`,
//! `std::time::Instant` or `SystemTime::now()` directly (tests may: they run
//! natively).
//!
//! What a browser cannot do stays honest: a file call answers
//! `ErrorKind::Unsupported` instead of pretending. `tokio::sync` and
//! `tokio::select!` need no runtime and are used as they are everywhere.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    /// The monotonic clock.
    pub use std::time::Instant;

    /// The wall clock, now.
    pub fn system_now() -> std::time::SystemTime {
        std::time::SystemTime::now()
    }

    pub use tokio::spawn;

    pub mod task {
        pub use tokio::task::{spawn_blocking, AbortHandle, Id, JoinError, JoinHandle, JoinSet};
    }

    pub mod time {
        pub use tokio::time::error::Elapsed;
        pub use tokio::time::{interval, sleep, timeout, timeout_at, Instant, Interval, MissedTickBehavior};
    }

    pub mod fs {
        pub use tokio::fs::*;

        /// What `metadata` answers (a browser has its own, with the same questions).
        pub type Metadata = std::fs::Metadata;

        // What the core asks without waiting. Here they are the std calls
        // it always made; in a browser they are answered from an index.

        /// What is at `path`, not following a link (`symlink_metadata`).
        pub fn metadata_now(path: impl AsRef<std::path::Path>) -> std::io::Result<Metadata> {
            std::fs::symlink_metadata(path)
        }
        pub fn exists_now(path: impl AsRef<std::path::Path>) -> bool {
            path.as_ref().exists()
        }
        /// One entry of a directory.
        pub struct Entry {
            pub path: std::path::PathBuf,
            pub name: String,
            pub metadata: Metadata,
        }
        /// The entries directly inside a directory. A name that is not
        /// UTF-8 is given as std's lossy text.
        pub fn read_dir_now(path: impl AsRef<std::path::Path>) -> std::io::Result<Vec<Entry>> {
            let mut all = Vec::new();
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                all.push(Entry { path: entry.path(), name: entry.file_name().to_string_lossy().into_owned(), metadata: entry.metadata()? });
            }
            Ok(all)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

    /// The monotonic clock (`performance.now()`): `std`'s panics in a browser.
    pub use web_time::Instant;

    /// The wall clock, now (`Date.now()`): `SystemTime::now()` panics in a
    /// browser, but a `SystemTime` built from the epoch is an ordinary value.
    pub fn system_now() -> std::time::SystemTime {
        let ms = js_sys::Date::now();
        std::time::UNIX_EPOCH + Duration::from_millis(if ms.is_finite() && ms > 0.0 { ms as u64 } else { 0 })
    }

    /// Start a task on the browser's own queue. There is one thread, so the
    /// future needs no `Send`.
    pub fn spawn<F>(fut: F) -> task::JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        task::spawn_with_id(task::next_id(), fut)
    }

    pub mod task {
        use super::*;
        use futures_util::future::{AbortHandle as Aborter, Abortable};
        use std::sync::atomic::{AtomicU64, Ordering};

        /// A task's number, unique for the life of the page.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct Id(u64);

        impl std::fmt::Display for Id {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        pub(super) fn next_id() -> Id {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            Id(NEXT.fetch_add(1, Ordering::Relaxed))
        }

        /// Why a task gave no result. A panic ends the whole page in a
        /// browser, so the only reason left is that it was aborted.
        #[derive(Debug)]
        pub struct JoinError {
            id: Id,
        }

        impl JoinError {
            pub fn id(&self) -> Id {
                self.id
            }
            pub fn is_cancelled(&self) -> bool {
                true
            }
            pub fn is_panic(&self) -> bool {
                false
            }
        }

        impl std::fmt::Display for JoinError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "task {} was cancelled", self.id)
            }
        }

        impl std::error::Error for JoinError {}

        /// Stops its task at the task's next wait.
        #[derive(Clone, Debug)]
        pub struct AbortHandle {
            id: Id,
            aborter: Aborter,
        }

        impl AbortHandle {
            pub fn abort(&self) {
                self.aborter.abort();
            }
            pub fn id(&self) -> Id {
                self.id
            }
        }

        /// A started task: await it for its result; dropping it lets the task
        /// run on (as tokio's does).
        pub struct JoinHandle<T> {
            id: Id,
            aborter: Aborter,
            done: tokio::sync::oneshot::Receiver<Result<T, JoinError>>,
            finished: bool,
        }

        impl<T> JoinHandle<T> {
            pub fn abort(&self) {
                self.aborter.abort();
            }
            pub fn abort_handle(&self) -> AbortHandle {
                AbortHandle { id: self.id, aborter: self.aborter.clone() }
            }
            pub fn id(&self) -> Id {
                self.id
            }
            pub fn is_finished(&self) -> bool {
                self.finished || !self.done.is_empty() || self.done.is_terminated()
            }
        }

        impl<T> Unpin for JoinHandle<T> {}

        impl<T> Future for JoinHandle<T> {
            type Output = Result<T, JoinError>;
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let id = self.id;
                match Pin::new(&mut self.done).poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(out) => {
                        self.finished = true;
                        // a sender dropped without a word: the task was torn down
                        Poll::Ready(out.unwrap_or(Err(JoinError { id })))
                    }
                }
            }
        }

        pub(super) fn spawn_with_id<F>(id: Id, fut: F) -> JoinHandle<F::Output>
        where
            F: Future + 'static,
            F::Output: 'static,
        {
            let (tx, done) = tokio::sync::oneshot::channel();
            let (aborter, registration) = Aborter::new_pair();
            let fut = Abortable::new(fut, registration);
            wasm_bindgen_futures::spawn_local(async move {
                let out = fut.await.map_err(|_| JoinError { id });
                let _ = tx.send(out);
            });
            JoinHandle { id, aborter, done, finished: false }
        }

        /// Work that would block a thread elsewhere. A browser has the one
        /// thread: the work runs here and now, and its handle is ready.
        pub fn spawn_blocking<F, R>(work: F) -> JoinHandle<R>
        where
            F: FnOnce() -> R + 'static,
            R: 'static,
        {
            let id = next_id();
            let (tx, done) = tokio::sync::oneshot::channel();
            let (aborter, _registration) = Aborter::new_pair();
            let _ = tx.send(Ok(work()));
            JoinHandle { id, aborter, done, finished: false }
        }

        /// Tasks that end together with their owner: dropping the set aborts
        /// what is still running (as tokio's does).
        pub struct JoinSet<T> {
            tasks: Vec<JoinHandle<T>>,
        }

        impl<T> Default for JoinSet<T> {
            fn default() -> Self {
                Self { tasks: Vec::new() }
            }
        }

        impl<T: 'static> JoinSet<T> {
            pub fn new() -> Self {
                Self::default()
            }
            pub fn len(&self) -> usize {
                self.tasks.len()
            }
            pub fn is_empty(&self) -> bool {
                self.tasks.is_empty()
            }
            pub fn spawn<F>(&mut self, fut: F) -> AbortHandle
            where
                F: Future<Output = T> + 'static,
            {
                let handle = spawn_with_id(next_id(), fut);
                let abort = handle.abort_handle();
                self.tasks.push(handle);
                abort
            }
            pub fn abort_all(&mut self) {
                for t in &self.tasks {
                    t.abort();
                }
            }
            fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<(Id, T), JoinError>>> {
                if self.tasks.is_empty() {
                    return Poll::Ready(None);
                }
                for i in 0..self.tasks.len() {
                    if let Poll::Ready(out) = Pin::new(&mut self.tasks[i]).poll(cx) {
                        let id = self.tasks.swap_remove(i).id;
                        return Poll::Ready(Some(out.map(|v| (id, v))));
                    }
                }
                Poll::Pending
            }
            /// The next task to end, with its number; None when none is left.
            pub async fn join_next_with_id(&mut self) -> Option<Result<(Id, T), JoinError>> {
                std::future::poll_fn(|cx| self.poll_next(cx)).await
            }
            pub async fn join_next(&mut self) -> Option<Result<T, JoinError>> {
                self.join_next_with_id().await.map(|r| r.map(|(_, v)| v))
            }
            /// A task that has already ended, without waiting.
            pub fn try_join_next_with_id(&mut self) -> Option<Result<(Id, T), JoinError>> {
                let waker = futures_util::task::noop_waker();
                match self.poll_next(&mut Context::from_waker(&waker)) {
                    Poll::Ready(out) => out,
                    Poll::Pending => None,
                }
            }
            pub fn try_join_next(&mut self) -> Option<Result<T, JoinError>> {
                self.try_join_next_with_id().map(|r| r.map(|(_, v)| v))
            }
            pub async fn shutdown(&mut self) {
                self.abort_all();
                while self.join_next().await.is_some() {}
            }
        }

        impl<T> Drop for JoinSet<T> {
            fn drop(&mut self) {
                for t in &self.tasks {
                    t.abort();
                }
            }
        }
    }

    pub mod time {
        use super::*;

        /// The clock timers run on (the monotonic one).
        pub use web_time::Instant;

        /// A browser timer takes its wait as a 32-bit count of milliseconds.
        fn millis(d: Duration) -> u32 {
            d.as_millis().min(i32::MAX as u128) as u32
        }

        /// Wait `d` (`setTimeout`).
        pub async fn sleep(d: Duration) {
            gloo_timers::future::TimeoutFuture::new(millis(d)).await
        }

        /// The wait ran out before the work was done.
        #[derive(Debug, PartialEq, Eq)]
        pub struct Elapsed(());

        impl std::fmt::Display for Elapsed {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("deadline has elapsed")
            }
        }

        impl std::error::Error for Elapsed {}

        /// `work`, given up after `d`.
        pub async fn timeout<F: Future>(d: Duration, work: F) -> Result<F::Output, Elapsed> {
            let work = std::pin::pin!(work);
            let wait = std::pin::pin!(sleep(d));
            match futures_util::future::select(work, wait).await {
                futures_util::future::Either::Left((out, _)) => Ok(out),
                futures_util::future::Either::Right(_) => Err(Elapsed(())),
            }
        }

        /// `work`, given up at `deadline`.
        pub async fn timeout_at<F: Future>(deadline: Instant, work: F) -> Result<F::Output, Elapsed> {
            timeout(deadline.saturating_duration_since(Instant::now()), work).await
        }

        /// What an interval does about ticks it missed. A browser's interval
        /// here always waits a whole period after the tick it gave (tokio's
        /// `Delay`), which is the only behaviour the core asks for.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum MissedTickBehavior {
            Burst,
            Delay,
            Skip,
        }

        /// Ticks `period` apart; the first comes at once (as tokio's does).
        pub struct Interval {
            period: Duration,
            first: bool,
        }

        pub fn interval(period: Duration) -> Interval {
            Interval { period, first: true }
        }

        impl Interval {
            pub fn set_missed_tick_behavior(&mut self, _behavior: MissedTickBehavior) {}
            pub async fn tick(&mut self) -> Instant {
                if self.first {
                    self.first = false;
                } else {
                    sleep(self.period).await;
                }
                Instant::now()
            }
        }
    }

    /// Files: Hubchat Web's own, in the origin's private file system
    /// (`rt_fs_web.rs`, over `webfs` and `rt_opfs`). The web shell awaits
    /// `fs::init` before it starts the core; before that, and in a browser
    /// without such storage, every call answers `ErrorKind::Unsupported`.
    pub use crate::rt_fs_web as fs;
}

#[cfg(target_arch = "wasm32")]
pub use web::*;
