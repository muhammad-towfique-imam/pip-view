//! A job on its own thread, with a time limit for the caller. A camera call
//! can block for a long time in a driver; the UI never waits for it longer
//! than its limit.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;

/// The number of running jobs for each name.
static ALIVE: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

fn count(name: &str, delta: isize) {
    let mut guard = ALIVE.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let n = map.entry(name.to_string()).or_insert(0);
    *n = n.saturating_add_signed(delta);
}

/// The number of jobs with this name that have not ended, also detached ones.
pub fn alive(name: &str) -> usize {
    let guard = ALIVE.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().and_then(|m| m.get(name).copied()).unwrap_or(0)
}

/// The job did not end in the time limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedOut;

/// Decreases the alive count when the job thread ends, also after a panic.
struct AliveGuard(String);

impl Drop for AliveGuard {
    fn drop(&mut self) {
        count(&self.0, -1);
    }
}

pub struct Job<T> {
    result: Receiver<T>,
}

impl<T: Send + 'static> Job<T> {
    /// Runs `f` on a new thread with the name `name`.
    pub fn spawn(name: &str, f: impl FnOnce() -> T + Send + 'static) -> Job<T> {
        let (tx, result) = mpsc::channel();
        count(name, 1);
        let guard = AliveGuard(name.to_string());
        let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || {
            let _guard = guard;
            let _ = tx.send(f());
        });
        if spawned.is_err() {
            // The guard went with the closure and is dropped: the count is correct.
            crate::log::log!("cannot start the thread {name}");
        }
        Job { result }
    }

    /// The result, if the job has ended. It comes 1 time.
    pub fn poll(&self) -> Option<T> {
        self.result.try_recv().ok()
    }

    /// Waits at most `limit` for the result.
    pub fn wait(&self, limit: Duration) -> Result<T, TimedOut> {
        match self.result.recv_timeout(limit) {
            Ok(v) => Ok(v),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Err(TimedOut),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn poll_gets_result() {
        let job = Job::spawn("test-poll", || 42);
        let t = Instant::now();
        let mut got = None;
        while got.is_none() && t.elapsed() < Duration::from_secs(2) {
            got = job.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some(42));
        assert_eq!(job.poll(), None, "the result comes 1 time");
    }

    #[test]
    fn wait_times_out_on_slow_job() {
        let job = Job::spawn("test-slow", || std::thread::sleep(Duration::from_secs(2)));
        let t = Instant::now();
        assert_eq!(job.wait(Duration::from_millis(100)), Err(TimedOut));
        assert!(t.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn wait_gets_result() {
        let job = Job::spawn("test-wait", || "ok");
        assert_eq!(job.wait(Duration::from_secs(2)), Ok("ok"));
    }

    #[test]
    fn alive_counts_running_jobs() {
        let job = Job::spawn("test-alive", || std::thread::sleep(Duration::from_millis(200)));
        assert_eq!(alive("test-alive"), 1);
        job.wait(Duration::from_secs(2)).unwrap();
        let t = Instant::now();
        while alive("test-alive") != 0 && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(alive("test-alive"), 0);
    }
}
