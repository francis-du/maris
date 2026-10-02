//! Normalized process-capture block assembly, independent of native handles.
//! Partial consumption retains tails; no stream is averaged or silently time-compressed.
use anyhow::{ensure, Result};
use std::time::{Duration, Instant};

pub(super) struct Block {
    samples: Vec<f32>,
    cursor: usize,
}
impl Block {
    pub fn new(samples: Vec<f32>, frames: usize) -> Result<Self> {
        ensure!(
            (1..=48_000).contains(&frames) && frames.checked_mul(2) == Some(samples.len()),
            "Invalid normalized Windows capture block"
        );
        Ok(Self { samples, cursor: 0 })
    }
    fn remaining(&self) -> usize {
        self.samples.len() / 2 - self.cursor
    }
    fn frame(&mut self) -> [f32; 2] {
        let offset = self.cursor * 2;
        self.cursor += 1;
        [self.samples[offset], self.samples[offset + 1]].map(|x| {
            if x.is_finite() {
                x.clamp(-16.0, 16.0)
            } else {
                0.0
            }
        })
    }
}
pub(super) fn mix(
    pending: &mut [Option<Block>],
    last_progress: &mut Instant,
    now: Instant,
    mut emit: impl FnMut([f32; 2]),
) -> Result<usize> {
    ensure!(
        !pending.is_empty() && pending.len() <= 64,
        "Invalid Windows capture source count"
    );
    if pending.iter().any(Option::is_none) {
        ensure!(
            now.saturating_duration_since(*last_progress) < Duration::from_secs(3),
            "Windows capture stopped delivering normalized audio"
        );
        return Ok(0);
    }
    let frames = pending
        .iter()
        .flatten()
        .map(Block::remaining)
        .min()
        .unwrap_or(0);
    for _ in 0..frames {
        let mut mixed = [0.0_f32; 2];
        for block in pending.iter_mut().flatten() {
            let frame = block.frame();
            mixed[0] += frame[0];
            mixed[1] += frame[1];
        }
        emit(mixed.map(|sample| sample.clamp(-16.0, 16.0)));
    }
    for block in pending {
        if block.as_ref().is_some_and(|b| b.remaining() == 0) {
            *block = None;
        }
    }
    *last_progress = now;
    Ok(frames)
}

#[cfg(test)]
#[path = "../../tests/unit/windows_mix.rs"]
mod tests;
