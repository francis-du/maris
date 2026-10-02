//! Ownership for workers whose initialization can fail or outlive its handshake.
use anyhow::{Context, Result};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start<S>(
        name: &str,
        timeout: Duration,
        initialize: impl FnOnce(&AtomicBool) -> Result<S> + Send + 'static,
        run: impl FnOnce(S, &AtomicBool) + Send + 'static,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let state = match initialize(&thread_stop) {
                    Ok(state) => state,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                // A late startup must release its resources, not become an orphan.
                if thread_stop.load(Ordering::Acquire) || ready_tx.send(Ok(())).is_err() {
                    return;
                }
                run(state, &thread_stop);
            })
            .with_context(|| format!("Spawn {name}"))?;
        // Establish the owner BEFORE waiting: every error cancels and joins.
        let worker = Self {
            stop,
            thread: Some(thread),
        };
        ready_rx
            .recv_timeout(timeout)
            .with_context(|| format!("{name} startup timed out or disconnected"))??;
        Ok(worker)
    }
    pub fn healthy(&self) -> bool {
        !self.stop.load(Ordering::Acquire)
            && self
                .thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // An OS call already in progress still has to return. The handshake timeout
        // is not a promise that an uninterruptible platform API can be force-killed.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/worker.rs"]
mod tests;
