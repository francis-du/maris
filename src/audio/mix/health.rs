//! Runtime checks and immediate failure containment on the control thread.
use super::super::Metrics;
use anyhow::{ensure, Result};
use std::sync::{atomic::Ordering, Arc};

pub(super) fn check(
    inputs: &[Arc<Metrics>],
    outputs: &[Arc<Metrics>],
    workers_healthy: impl FnOnce() -> bool,
    validate: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let result = (|| {
        validate()?;
        // A successful startup does not prove a worker survived later blocks.
        // Check after native validation, which may block while a worker exits.
        ensure!(
            workers_healthy(),
            "Mixer speech worker stopped; restoring original playback"
        );
        for output in outputs {
            ensure!(
                output.errors.load(Ordering::Acquire) == 0,
                "Mixer output stream failed"
            );
        }
        ensure!(
            inputs.iter().all(|m| m.errors.load(Ordering::Acquire) == 0),
            "Mixer input stream failed"
        );
        Ok(())
    })();
    if result.is_err() {
        // Returning Err need not immediately drop the owning session. Stop PCM
        // now; teardown still owns restoration and releases capture before output.
        for output in outputs {
            output.output_quarantined.store(true, Ordering::Release);
        }
    }
    result
}

/// Finalize one control tick; the owning session retains teardown responsibility.
pub(super) fn finish_tick(
    result: Result<bool>,
    stopped: &mut bool,
    outputs: &[Arc<Metrics>],
) -> Result<bool> {
    if result.is_err() {
        // State reads and status writes can fail after the native health check.
        // Contain every fatal return even when the caller retains this session.
        *stopped = true;
        for output in outputs {
            output.output_quarantined.store(true, Ordering::Release);
        }
    }
    // Successful control notices and graceful stops retain their existing fade.
    result
}

#[cfg(test)]
#[path = "../../../tests/mixing/health.rs"]
mod tests;
