//! Control-thread capture sequencing. Session retains ownership on every error.
use super::super::Metrics;
use anyhow::{ensure, Result};
use std::sync::{atomic::Ordering, Arc};

pub(super) trait CaptureLifecycle {
    fn start(&mut self) -> Result<()>;
    fn check(&mut self) -> Result<()>;
    fn arm(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(super) fn start<C: CaptureLifecycle>(
    captures: &mut [C],
    outputs: &[&Metrics],
    two_phase: bool,
    mut validate: impl FnMut() -> Result<()>,
) -> Result<()> {
    let check_outputs = || -> Result<()> {
        ensure!(!outputs.is_empty(), "Missing mixer output");
        for output in outputs {
            ensure!(
                output.errors.load(Ordering::Acquire) == 0,
                "Mixer output failed during capture startup"
            );
            ensure!(
                output.callback_ready.load(Ordering::Acquire),
                "Mixer output callback is not ready"
            );
            ensure!(
                output.output_quarantined.load(Ordering::Acquire),
                "Mixer preflight output is not quarantined"
            );
        }
        Ok(())
    };
    if two_phase {
        check_outputs()?;
    }
    validate()?;
    for capture in captures.iter_mut() {
        capture.start()?;
        if two_phase {
            check_outputs()?;
        }
    }
    if !two_phase {
        return Ok(());
    }
    // Native macOS taps start unmuted. Prove every capture and revalidate the
    // user's revision before muting even the first original application path.
    validate()?;
    for capture in captures.iter_mut() {
        capture.check()?;
        check_outputs()?;
    }
    for capture in captures.iter_mut() {
        capture.arm()?;
        check_outputs()?;
    }
    // Arming can block in native code while revisions or stream errors change.
    // Recheck after the last arm, not only before the first native mute update.
    validate()?;
    check_outputs()?;
    // Any failure above leaves all outputs quarantined. The owning Session
    // restores every armed capture before releasing any replacement stream.
    for output in outputs {
        output.output_quarantined.store(false, Ordering::Release);
    }
    Ok(())
}

pub(super) fn release<C, S>(captures: &mut Vec<C>, streams: &mut Vec<S>, outputs: &[Arc<Metrics>]) {
    // A stalled callback can miss the bounded fade wait. Do not let it resume
    // processed audio after the original application path has been restored.
    for output in outputs {
        output.output_quarantined.store(true, Ordering::Release);
    }
    captures.clear();
    streams.clear();
}

/// Prove capture and rendering after takeover, not just silent preflight callbacks.
#[cfg(any(target_os = "macos", test))]
pub(super) fn wait_ready(
    inputs: &[&Metrics],
    outputs: &[&Metrics],
    timeout: std::time::Duration,
    mut validate: impl FnMut() -> Result<()>,
) -> Result<()> {
    // Captured counters alone can include preflight PCM; renderer flags alone
    // can reflect buffered data. Require new capture AND each input's own graph
    // acknowledgement, plus source progress on every active physical output.
    let before: Vec<_> = inputs
        .iter()
        .map(|m| m.captured_frames.load(Ordering::Acquire))
        .collect();
    let result = (|| {
        ensure!(
            !inputs.is_empty() && !outputs.is_empty(),
            "Missing mixer handoff inputs or outputs"
        );
        let deadline = std::time::Instant::now() + timeout;
        loop {
            validate()?;
            for metrics in inputs.iter().chain(outputs) {
                ensure!(
                    metrics.errors.load(Ordering::Acquire) == 0,
                    "Mixer stream failed after application takeover"
                );
                ensure!(
                    !metrics.stopping.load(Ordering::Acquire),
                    "Mixer stopped during application takeover"
                );
            }
            ensure!(
                outputs
                    .iter()
                    .all(|m| !m.output_quarantined.load(Ordering::Acquire)),
                "Mixer output was quarantined during application takeover"
            );
            let inputs_ready = inputs.iter().zip(&before).all(|(m, before)| {
                m.captured_frames.load(Ordering::Acquire) != *before
                    && m.source_started.load(Ordering::Acquire)
                    && m.configuration_events.load(Ordering::Acquire)
                        == m.checked_configuration_events.load(Ordering::Acquire)
            });
            let outputs_ready = outputs.iter().all(|m| {
                m.callback_ready.load(Ordering::Acquire)
                    && m.source_started.load(Ordering::Acquire)
                    && m.configuration_events.load(Ordering::Acquire)
                        == m.checked_configuration_events.load(Ordering::Acquire)
            });
            if inputs_ready && outputs_ready {
                return Ok(());
            }
            ensure!(std::time::Instant::now() < deadline,
                "Mixer received no ready post-takeover source on every input/output; restoring original playback");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        // Do not wait for Session's fade timeout before blocking a failed graph.
        // Session still owns all captures and restores them before dropping outputs.
        for output in outputs {
            output.output_quarantined.store(true, Ordering::Release);
        }
    }
    result
}

#[cfg(test)]
#[path = "../../../tests/mixing/handoff.rs"]
mod handoff_tests;

#[cfg(test)]
#[path = "../../../tests/mixing/startup.rs"]
mod tests;
